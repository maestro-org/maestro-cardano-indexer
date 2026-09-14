# polyphony

The **indexer**: consumes enriched blocks from [compressor](../compressor)
over gRPC, runs them through a configurable set of 17 **reducers**
(map/reduce-style folds, one per query pattern), and writes the resulting
key/value mutations to TiKV — plus cursor and instance-registry entries to
Redis so the API layer knows what data exists and how fresh it is.

Began as a fork of TxPipe's [Scrolls](https://github.com/txpipe/scrolls) and
has since been substantially rewritten; built on the
[gasket](https://github.com/construkts/gasket-rs) staged-pipeline framework:

```
source (compressor gRPC / emulator) ──► reducers (fan-out per block) ──► storage (TiKV + Redis)
```

## Key properties

- **Multi-instance by design.** Each instance is identified by
  (`dataplane_id`, `instance_id`) — both `u8` — and namespaces every TiKV key it
  writes with that identity (the key layout lives in
  [timbre](../../crates/timbre)). Any number of instances — different reducer
  sets, different code versions — can index the same chain into the same TiKV
  cluster concurrently, all fed by one compressor.
- **Registry-gated swap-over.** An instance advertises itself in the per-reducer
  Redis instance registry (`cardano:<network>:<name>:scores`) **only once it
  reaches the mutable window at the chain tip**. Until then the API layer
  doesn't know it exists. This is the zero-downtime upgrade mechanism: deploy a
  new instance with patched reducers, let it backfill in parallel, and it takes
  over automatically when caught up. Set
  `storage.advertise_immediately = true` to bypass the gate in dev setups.
- **Rollback-aware.** Inverse actions for the most recent 32 blocks are
  buffered in memory and persisted to TiKV (surviving restarts), so chain
  rollbacks unwind cleanly; a rollback deeper than the buffer stops the
  instance rather than letting it serve wrong data (the `[policy]` section can
  relax individual error classes).
- **Crash-safe commits.** Each commit runs as an in-memory two-phase work unit
  — re-scheduled and re-executed if the process dies mid-commit — and a block's
  actions and cursor advance in one TiKV transaction. Optional
  `storage.paranoid_mode` reads every mutation back after commit and verifies
  it landed.
- **Batched backfill.** While processing immutable history, the actions of up
  to `sync_batch_size` blocks are algebraically merged into one transaction,
  flushed early when the batch approaches TiKV's raft-entry-size limit.

## Running

```
polyphony daemon --config <config.toml> [--console plain|tui]
```

## Configuration

Layered: `/etc/polyphony/daemon.toml`, then `polyphony.toml` in the working
directory, then `--config <file>`, then env vars prefixed `POLYPHONY` with `__`
separator (e.g. `POLYPHONY__STORAGE__INSTANCE_ID=1`). Complete examples in
[`configs/polyphony/`](../../configs/polyphony).

| Section | Keys |
|---|---|
| `[source]` | `type = "Compressor"` with `url`, `max_items_per_page` — or `type = "Emulate"` with synthetic `instructions` (`F` = apply next block, `Bn` = roll back `n` blocks) for testing |
| `[intersect]` | Where to start: `type = "Origin"`, `type = "Tip"`, or `type = "Point"` with `value = [slot, "hash"]` |
| `[storage]` | `type = "TiKV"`: `connection_params` (PD endpoint), `redis_address`, `network` (`mainnet`/`preprod`/`preview`), `dataplane_id`, `instance_id`, `advertise_immediately`, plus commit tuning (`sync_batch_size`, `tikv_raft_entry_max_size`, `paranoid_mode`) |
| `[[reducers]]` | One `type = "..."` entry per reducer to run — see [docs/reducers.md](../../docs/reducers.md) |
| `[policy]` | Optional error handling per class (`missing_data`, `cbor_errors`, `ledger_errors`, `rollback_errors`, `any_error`); unset classes are fatal |

polyphony exposes no network listener of its own; observe it through its logs,
its Redis cursor entries, or the TUI console.
