# Design

This stack indexed Cardano in production at [Maestro](https://www.gomaestro.org/).
This document explains how the pieces fit together and, more importantly, *why*
the system is shaped the way it is: the problems that come up operating a
multi-tenant Cardano indexing platform, and the mechanisms built to solve them.

## The stack in one pass

```
cardano-node ──socket──► compressor ──gRPC──► polyphony (×N) ──► TiKV + Redis ◄── mapi ──HTTP──► clients
                                                                      ▲              ▲
                                                                   tikv-gc     db-sync / Ogmios
```

**cardano-node** is a stock node. Nothing in the stack patches or extends it; it
is only the source of blocks, reached over the node socket via the Ouroboros
chainsync mini-protocol (node-to-client, through
[Pallas](https://github.com/txpipe/pallas)).

**compressor** follows the node and maintains its own RocksDB store. It is not
a block cache — its job is *enrichment*. A raw Cardano block does not carry the
information an indexer actually needs: inputs are just pointers to outputs of
older transactions. Compressor resolves every input to its full previous output
— seeding the Byron genesis UTxOs from the genesis file so resolution is
complete from slot zero — and serves *enriched blocks* (raw blocks plus a
resolver map of every consumed output, each tagged with the era needed to
decode it) over gRPC: as paged history for backfills, and as a follow-the-tip
stream with apply/undo semantics for rollbacks.

Why does compressor exist at all — why not point the indexer at the node?
Because resolving inputs requires a complete TXO index built by processing the
whole chain. If each indexer did this itself, every instance would carry many
gigabytes of resolver state and hours-to-days of extra sync time — and the
stack's operational model (§1) depends on indexer instances being cheap enough
to create and discard freely. Compressor pays the expensive cost exactly once
and amortises it over any number of consumers. It is also the only component
that has to care about chainsync intersection, era decoding quirks, and
rollback detection; everything downstream sees one clean, ordered,
rollback-aware stream.

**polyphony** is the indexer — a three-stage
[gasket](https://github.com/construkts/gasket-rs) pipeline: a *source* (the
compressor gRPC client, or a synthetic emulator for testing), a *reducer* stage
fanning each block out to a configurable set of reducers — functions that fold
each block into key/value updates (UTxOs by address, asset supplies,
transaction CBOR, policy activity, …) — and a *storage* stage. Reducer output is
expressed in a small storage DSL (§4) that the storage stage merges and batches
before committing to TiKV. Each polyphony process is an *instance* identified by
`(dataplane_id, instance_id)`; every key it writes is namespaced by that
identity, and after every commit it publishes a cursor entry to Redis recording
the slot, block hash, and TiKV commit timestamp. Instances are deliberately
disposable: many can run against one compressor, indexing in parallel, with
completely independent keyspaces.

**timbre** is not a service but a library — the byte-level definition of every
key and value the stack stores. It is the schema contract: polyphony links it to
write, mapi links it to read, and the two never communicate directly. Data is
the interface, and §9 explains how the key layout itself is what makes a plain
key/value store queryable.

**mapi** is the stateless HTTP API. On each request it decides *which instance's
data to read*: it consults the per-reducer instance registry in Redis, opens a
TiKV snapshot at the current timestamp, and reads the selected instance's
keyspace — stamping the response with the block that instance had indexed to
(`last_updated`, read from the instance's own cursor in the same snapshot).
Endpoint groups that need ledger-derived state (accounts, pools, epochs) read
from a cardano-db-sync Postgres, and protocol parameters and Plutus cost models
come from an Ogmios websocket (§8).

**tikv-gc** closes the loop. TiKV is an MVCC store that keeps old versions of
values — until garbage collection. tikv-gc owns the GC safepoint: it computes
the newest timestamp that is provably safe to collect past (§2) and submits it
to TiKV's placement driver. Run it frequently and TiKV stays compact; run it
rarely and old snapshots stay readable longer. Without it, TiKV never collects
at all.

**Redis** is the coordination plane for all of the above: cursor entries, the
instance registry, and GC bookkeeping. It holds no indexed data; a lost Redis is
repopulated by the running instances.

### Why TiKV

Four properties drove the choice of store, and the design leans on all of them:

- **MVCC snapshots.** Every read happens at a well-defined timestamp, and old
  versions stay readable until garbage collection is explicitly allowed to pass
  them (§2). The stack never built its own snapshotting.
- **Horizontal write scaling.** Many indexer instances commit concurrently;
  region-sharded TiKV scales writes by adding nodes rather than by sharding in
  application code.
- **Ordered keys and range scans.** The entire query model is prefix and range
  scans over carefully constructed keys (§9). A sorted KV store matches it
  exactly; nothing needs secondary indexes.
- **Multi-tenancy in one cluster.** Key-prefix namespacing gives every network,
  environment, and instance its own contiguous keyspace in a shared cluster —
  and retiring an instance is a single range deletion.

## Components at a glance

| Component | Kind | Reads | Writes | Why it exists |
|---|---|---|---|---|
| cardano-node | stock node | the network | — | source of truth |
| compressor | service (Rust) | node socket (chainsync N2C) | its RocksDB; serves gRPC | resolve inputs **once**, serve enriched blocks to N consumers |
| polyphony | service (Rust), ×N instances | compressor gRPC | TiKV (data), Redis (cursors/registry) | fold blocks into queryable k/v via reducers; disposable per-instance |
| timbre | library (Rust) | — | — | the key/value schema contract between writer and reader |
| mapi | service (Rust) | TiKV, Redis, db-sync Postgres, Ogmios | — | stateless API; per-request instance selection |
| tikv-gc | job (Rust) | Redis | TiKV (PD safepoint) | make MVCC retention explicit: collect exactly what no reader can need |

---

# Problems and the mechanisms that solve them

## 1. Fixing a broken indexer with zero downtime

Indexers have bugs. A reducer that mis-handled an edge case has, by the time the
bug is noticed, written wrong values into the store — and the wrong values are
entangled with months of correct ones. The traditional answers are bad: take the
API down and re-index, or serve known-bad data while a fix crawls through.

The stack dissolves the problem with three decisions that work together. First,
indexed data is *derived and disposable* — nothing in TiKV is precious, because
it can always be rebuilt from the chain. Second, an indexer instance's identity
is its `(dataplane_id, instance_id)` pair, and every key it writes is prefixed
with that identity — so two instances of the same reducer coexist in the same
cluster without touching each other. Third, *which instance the API reads* is
not configuration but data: a Redis sorted set per instance name
(`cardano:<network>:<instance-name>:scores`), whose members are 2-byte instance
identities and whose scores are the slot each instance has indexed to. Most
reducers register under their own name; five asset/policy reducers
(`Cip25MetadataByAsset`, `MintMetadataByAsset`, `ScriptByHash`,
`SupplyByAsset`, `UpdatesByPolicy`) are grouped under one `assets-policies`
name, because the endpoints that read them cross those reducers freely.

Fixing a bug then becomes: patch the reducer, start a new instance with a fresh
`instance_id`, and let it re-index the chain in parallel with the live one. The
new instance is invisible while it backfills — instances only advertise
themselves in the registry once they enter the mutable window at the chain tip
— so the API keeps serving the old data uninterrupted. When the new instance
registers, mapi's per-request instance resolution starts selecting it. The old
instance is then retired: stopped, removed from the registry, its contiguous
keyspace deleted at leisure. No downtime, no flag day, no coordination beyond a
sorted-set entry.

Because the registry is per instance name, production ran one deployment per
reducer (or small group) per network — so a fix to one reducer never risked the
others. The compose file in this repository demonstrates the full cycle with
two instances (`polyphony-a`/`polyphony-b`): start the second at any time,
watch it backfill invisibly, and observe the API's instance selection move to
it once it registers at the tip.

Relevant code: registration in
`services/polyphony/src/storage/tikv.rs` (`maybe_advertise`,
`update_registry_scores`); resolution in
`services/mapi/src/key_resolver.rs` (`resolve_key`).

## 2. One store, many instances: MVCC and tikv-gc

Every polyphony commit records its TiKV commit timestamp against the block slot
in a Redis cursor entry, so "the state instance *i* had at slot *s*" is
precisely "a snapshot at that commit's timestamp". TiKV's MVCC keeps those
versions readable — until garbage collection. The stack turns *when old
versions may be collected* into an explicit contract rather than a default.

tikv-gc computes the greatest safepoint that preserves every timestamp a reader
could still need, as the minimum of three invariants
(`services/tikv-gc/src/invariants/`):

- **Earliest chaintip.** The safepoint never passes the commit timestamp of any
  instance's chaintip entry, so every instance's own tip-consistent snapshot
  stays readable. The job warns if this earliest chaintip is more than two
  hours stale, since a stalled instance pins GC for everyone.
- **Network intersection.** For each network, the highest block present in
  *all* instances' entries (and the intersection at the height below it) must
  remain readable — so a consistent common point across instances at slightly
  different slots always exists in the store.
- **Safe zone.** Nothing newer than a configurable window (default ten minutes)
  is ever collected, regardless of the other invariants — covering the lifetime
  of in-flight transactions and long-running reads.

The operational consequence is a pleasingly simple dial: **how long old MVCC
versions stay readable equals the GC cadence.** Run tikv-gc rarely and the
store retains deep version history at the cost of disk; run it every ten
minutes and TiKV stays compact. In this repository's compose stack it runs
continuously on a ten-minute loop; production ran the same binary as a CronJob.

## 3. One stream from Byron to Conway

Cardano's history spans eras with different block formats, address formats, and
transaction structures — Byron through Conway — and the ledger's initial UTxO
set is not on the chain at all: Byron genesis UTxOs exist only in the genesis
configuration file.

Compressor absorbs both problems so no consumer has to. At first start it seeds
its TXO store with the genesis UTxOs parsed from the Byron genesis JSON (the
`[byron]` config entry; the genesis files for mainnet, preprod, and preview
ship in `services/compressor/genesis/`), so input resolution is complete from
the first block. And every resolved output it serves carries three things: the
raw CBOR of the output, the slot of the block that produced it, and its **era
tag** — so a consumer handed a Conway transaction spending a Byron-era output
can decode both correctly without maintaining any era bookkeeping of its own
(`ResolvedTxo` in `proto/sync/v1/sync.proto`).

Downstream, reducers work on Pallas' multi-era block and output types, so a
single reducer implementation covers all of chain history — there is no
per-era reducer code to write or to drift out of sync.

One notable non-feature: **there is no mempool in this stack**. Nothing
assembles or enriches pending transactions, and no endpoint is mempool-aware —
the pipeline indexes committed chain state only. (Transaction *submission* is
likewise out of scope; see §8.)

## 4. The storage DSL: actions as algebra

Reducers do not write to the database. They emit *actions* — `set`, `delete`,
`insert`, and u64/u128 `increment`/`decrement` variants — and the storage stage
owns what those actions become. This indirection buys three things.

It buys optimisation. Actions over the same key compose algebraically: a set
followed by a set is the last set; a set followed by a delete is nothing;
increments fold into a single sum. The sync batcher
(`services/polyphony/src/storage/tikv.rs`, `SyncBatcher`) applies these rules
across whole batches of blocks during backfill (§7), merging the raw action
stream down before anything reaches the store, with no reducer knowing or
caring.

It buys invertibility. Because every action is a small semantic operation rather
than an opaque write, each has a computable inverse (a set's inverse restores
the previous value; an increment's inverse is a decrement). The rollback
machinery of §5 is built entirely on this property.

And it buys decoupling: reducers stay pure folds from blocks to actions, unit
testable without a database, while batching, merging, retries, and commit
strategy live in one place.

## 5. Rollbacks: the rollback buffer and refusing to be wrong

Cardano rolls back. Ouroboros consensus regularly switches to a better fork
near the tip, and an indexer that has applied blocks which are no longer part
of the chain must *unapply* them — exactly, not approximately, because
downstream data (balances, UTxO sets) must be correct at every point.

As each block's actions are committed, polyphony also records their inverses in
a rollback buffer covering the last 32 blocks — held in memory and persisted to
TiKV under the rollback key plane (`services/polyphony/src/rollback/`,
restored by `fetch_persistent_buffer` on start-up), so it survives process
restarts. Handling a rollback is mechanical: apply the stored inverses back to
the fork point in one TiKV transaction, trim the persisted metadata, then apply
the new branch forward.

The deliberate design decision is what happens when a rollback exceeds the
buffer: the instance panics (the `[policy]` config can relax individual error
classes, but the default for an unhandleable rollback is to stop). It does not
skip, approximate, or resume from the fork point with stale keys from the
orphaned branch still in the store — it refuses to continue, because past the
buffer it can no longer guarantee correctness. The remedy is the standard
replacement flow of §1: start a fresh instance, backfill, swap over.

The 32-block buffer comfortably covers ordinary Ouroboros fork switches, which
are almost always a handful of blocks. Note the runway is also bounded by
compressor: the indexer switches to buffered (mutable) processing when it can
intersect compressor's mutable stream, and that intersect is only possible
within compressor's own mutable window (`immutable_after_slots`) — so the
effective rollback depth is the smaller of the two.

Rollback handling exists at both layers of the pipeline, and end to end it
runs: the node switches fork; compressor's chainsync stage receives the
rollback, rewinds its own store (it keeps a rollback log for the mutable
window of recent blocks), and emits explicit *undo* messages followed by the
replacement blocks on the gRPC stream; each polyphony instance applies its
buffered inverses in a TiKV transaction and indexes the new branch; and mapi
never notices — cursor entries only advance on committed state.

## 6. Resolve once, enrich everywhere

Section "The stack in one pass" gave the argument for compressor's existence;
this section gives the mechanics, because the enrichment contract is the
load-bearing wall of the whole design.

For every block, compressor ships: the raw block bytes, and a resolver map
covering every input the block spends — each entry the full previous output
(raw CBOR), the slot it was created at, and its era tag (§3). Computing this
requires a complete TXO index built by sequential processing of the whole
chain. By doing it once, behind a gRPC interface, the marginal cost of an
additional indexer instance drops to almost nothing: a fresh instance holds no
resolver state of its own and backfills an entire chain at whatever rate the
storage layer can absorb writes.

The gRPC contract (`proto/sync/v1/sync.proto`, package `compressor.sync.v1`)
has two surfaces: `PageBlocksWithContext` for bulk history (paged, for
backfills), and `StreamUpdatesWithContext` for the tip — a stream of
apply/undo/reset instructions, making rollbacks explicit protocol events rather
than something each consumer detects.

## 7. Committing safely: work units, batching, and paranoid mode

A commit must never be *half*-observed: if the process dies between writing a
block's data and advancing its cursor, a restart must land in a consistent
state.

The storage stage wraps every commit in an in-memory **two-phase work unit**
(`work_unit_2pc` in `services/polyphony/src/storage/tikv.rs`): the unit is
recorded before its TiKV transaction begins and cleared only after the
transaction commits. If anything fails in between, the gasket scheduler finds
the uncommitted unit on its next pass and re-executes it — and because the
block's actions and its cursor advance in *one* TiKV transaction, a failed
commit left nothing behind to clean up. Re-execution is idempotent by
construction.

Throughput during backfill comes from the **sync batcher**. While processing
immutable history (no rollback risk), the actions of up to `sync_batch_size`
blocks are accumulated and algebraically merged (§4) into a single transaction,
which is flushed early whenever its estimated size approaches TiKV's
raft-entry-size limit (`tikv_raft_entry_max_size`, default 4 MB) — large
transactions are fast, and never so large that TiKV rejects them. Once the
instance reaches the mutable window at the tip, batching stops and every block
commits (and registers its rollback inverses) individually.

For operators who want belt *and* braces, `paranoid_mode = true` makes the
worker read every mutation back from TiKV after each commit and verify the
stored value matches what was written, retrying until the store confirms — plus
a small verification key recording the last verified slot that is
cross-checked against the cursor on start-up. It trades commit latency for a
machine-checked guarantee that what the cursor claims is exactly what the store
contains.

## 8. Reading: per-request instance resolution

An API request needs two decisions made for it: *whose* data to read (which
instance currently serves each reducer), and *what else* the response needs
that the indexed keyspace cannot provide.

For the first, mapi consults the instance registry on every request: the first
member of `cardano:<network>:<instance-name>:scores` names the
`(dataplane_id, instance_id)` whose keyspace serves that reducer
(`services/mapi/src/key_resolver.rs`). Which instance serves is therefore data,
not configuration — the swap-over of §1 needs no API restart. The read itself
opens a TiKV snapshot at the current timestamp, and the response reports
`last_updated` — the block the serving instance had indexed to, read from that
instance's own cursor key *in the same snapshot as the data*, so the stamp and
the data always agree.

Honest scoping, and a deliberate contrast with the Bitcoin sibling: this stack
does **not** unify a request's reads onto one common block across reducers, and
does not serve time-travel queries — each response reflects the serving
instance's state at read time. In practice instances at the tip are within a
block of each other, and each response names exactly where it was read from.

For the second decision, three endpoint groups delegate to auxiliary backends:

- **cardano-db-sync (Postgres)** — accounts, pools, epochs, and era/system
  metadata: state derived from the ledger rules (rewards, delegation, stake
  distribution) that a block-fold reducer cannot compute. The queries go
  through the Koios SQL layer (the `grest` schema) installed into the db-sync
  database; the compose `full` profile sets this up automatically.
- **Ogmios (websocket)** — current protocol parameters and Plutus cost models,
  cached with a short TTL and single-flight refresh.
- **uplc (in-process)** — `/transactions/evaluate` runs phase-two Plutus
  evaluation locally against resolved inputs from the indexed keyspace and cost
  models from Ogmios, returning execution units per redeemer.

Transaction *submission* is deliberately out of scope: the API is read-only,
and submitting Cardano transactions is a separate concern (the node's
tx-submission protocol, Ogmios, or a dedicated submission service).

## 9. The key encoding: how a key/value store answers range queries

TiKV has no indexes, no query language, and no planner — only ordered bytes and
range scans. Timbre's job is to make byte order itself answer every query the
API needs.

Every data key follows one shape:

```
<dataplane u8><instance u8> 'D' <reducer tag> <key body…>
```

The 2-byte namespace makes multi-tenancy a property of the keyspace (§1); the
`'D'` tag separates data from the other planes that share the store — the
cursor (`'C'`), rollback entries (`'R'`), instance info (`'I'`), index (`'X'`),
and externally-ingested token-registry metadata (`'T'`) — so even the rollback
machinery of §5 lives in the same ordered keyspace. One byte then names the
reducer, and everything after belongs to that reducer's key body
(`crates/timbre/src/encoding/encode.rs`; the full tag table is in the
[timbre README](../crates/timbre/README.md)).

Two rules make the body scannable. All integers encode **big-endian**, so
lexicographic byte order coincides with numeric order — a scan over a byte range
*is* a scan over a slot range. And **field order is the query plan**: each
reducer's key orders its fields to match its endpoint's access pattern. The
UTxOs-by-address key is `{address, slot, utxo_hash, utxo_index}` — all UTxOs of
an address sit contiguously (a prefix scan), slot-ordered within, with the
outpoint serving only to disambiguate. There is no index because the schema
*is* the index; designing a reducer is designing its key order.

The `BREAK` byte (0x60) that delimits variable-length fields is what makes
prefix scans cheap to bound. To scan everything belonging to a prefix, the
range is simply `[<prefix><BREAK> .. <prefix><BREAK+1>)`: because `BREAK+1` is
the next byte value, the end key is the smallest byte string greater than
*every* key under the prefix — an exact exclusive upper bound obtained by
incrementing the delimiter rather than doing any arithmetic on the data itself.
For arbitrary prefixes without a trailing delimiter, the general fallback
increments the prefix's last non-`0xFF` byte (`prefix_key_range`,
`crates/timbre/src/encoding/namespace.rs`).

Pagination then falls out for free: a page cursor is simply (a suffix of) the
last key returned, base64url-encoded and handed to the client as an opaque
token. The next page resumes the scan at exactly that key — no offsets, no skip
cost, stable under concurrent writes. The odd-looking `next_cursor` strings in
API responses are these raw key bytes.

## 10. Smaller mechanisms worth knowing about

**Tip-gated registration.** An instance only advertises itself in the instance
registry once it reaches the mutable window — the point where compressor's
stream switches from immutable history to rollback-aware blocks (an
`advertise_immediately` config override exists for development setups). In
steady state — adding an instance to an already-synced deployment, the upgrade
scenario — the mutable window sits at the live network tip, so backfilling
instances are invisible until fully caught up. One nuance during a brand-new
deployment's *first* sync: the window tracks the upstream compressor's frontier,
which itself follows the node's initial sync — so the first instance advertises
early and the API serves immediately, with data completeness advancing as the
whole pipeline catches up to the network tip.

**Cursor entries as universal glue.** One record type — slot, block hash, TiKV
commit timestamp, published to Redis per commit — powers three unrelated
subsystems: mapi's freshness reporting (§8), tikv-gc's safepoint invariants
(§2), and the instance registry scores (§1). The stack coordinates through this
one small, append-only vocabulary.

**The emulate source.** Polyphony can run against a synthetic chain: the
`Emulate` source turns an instruction string into apply/undo events — `F`
applies the next canned block, `Bn` rolls back `n` blocks — so a config of
`"FFFB2FF"` scripts an arbitrary rollback scenario. Rollback logic is tested in
milliseconds with no node, no compressor, and no real chain.

**Retirement is a range delete.** Because an instance's entire output lives
under its 2-byte key prefix, decommissioning one is a registry removal plus a
single contiguous range deletion — no tombstone sweeps, no cross-key
bookkeeping.

---

# Appendix: the coordination records in Redis

Everything the stack coordinates through fits in two small Redis structures
(a plain single Redis instance — no cluster required).

**Cursor entries** — written by polyphony after every commit:

```
key:    tikv-timestamps:<dataplane_id>:<instance_id>     (sorted set)
score:  TiKV commit timestamp (physical ms)
member: slot,block_hash,false,commit_ts,network,slot,block_hash,0
```

plus `tikv-timestamps-keys`, a sorted set of those key names by most recent
activity. The member is comma-separated; the third field (`was_mempool`) is
always `false`, the sixth and seventh mirror the entry's own point as the chain
tip, and the eighth (`mempool_view_ts`) is always `0` — the format is shared
with the Bitcoin sibling's tooling, and the fields that only make sense there
are pinned to their empty values (`services/polyphony/src/storage/redis_entry.rs`).
These entries are what tikv-gc derives safepoints from (§2).

**Instance registry** — the swap-over switch (§1):

```
key:    cardano:<network>:<instance-name>:scores         (sorted set)
member: <dataplane_id: u8><instance_id: u8>              (2 bytes)
score:  slot this instance has indexed to
```

`<instance-name>` is the reducer's registry name — one per reducer, except the
five asset/policy reducers grouped as `assets-policies`
(`reducers::Config::instance_name` in
`services/polyphony/src/reducers/mod.rs`).

Registry entries are not garbage-collected: retiring an instance includes
removing its members (`ZREM`) — see [operations.md](operations.md) for the full
retirement runbook.
