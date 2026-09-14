# Operations

The docker compose file is a faithful miniature of the production topology this
stack ran in. Scaling it up is mostly a matter of multiplying instances, not
changing code.

## From compose to production

| Compose | Production shape |
|---|---|
| One or two polyphony instances running all 17 reducers each | One deployment **per reducer** (or small groups, like the `assets-policies` set), each with its own `instance_id` — independent backfills, restarts, and upgrades per reducer |
| pd + tikv single nodes | A real TiKV cluster (3+ PD, 3+ TiKV) — e.g. via tidb-operator on Kubernetes |
| single redis | A managed / replicated Redis (plain, not cluster — the services use single-node clients) |
| tikv-gc service looping every 10 min | A CronJob (e.g. every 10 minutes) |
| one compressor | Still one compressor (plus a warm standby if desired) — it is the only consumer of cardano-node and serves any number of indexers |
| mapi single replica | Stateless — scale horizontally behind a gateway (which is also where auth/rate limits belong) |
| `full`-profile postgres + cardano-db-sync + ogmios | Production-grade db-sync deployment and Ogmios instance(s), each following their own node |

## Identity planning

- `dataplane_id` (u8): one per network+environment (e.g. mainnet-prod = 0).
  mapi assumes instances within a dataplane are mutually consistent.
- `instance_id` (u8): unique per polyphony instance within a dataplane. Never
  reuse a live instance's id — keys are namespaced by it.

## Upgrading a reducer (zero downtime)

The rationale and mechanism are described in
[design.md §1](design.md#1-fixing-a-broken-indexer-with-zero-downtime); the runbook:

1. Deploy a new polyphony instance with the patched reducer and a fresh
   `instance_id`, `intersect` type `Origin` (or a snapshot point).
2. Watch it backfill (its cursor entries advance in Redis; it is *not* yet in the
   instance registry, so the API ignores it).
3. At the tip it registers itself; mapi's per-request instance resolution starts
   selecting it.
4. Retire the old instance: stop it, `ZREM` its 2-byte member from the
   `cardano:<network>:<instance-name>:scores` sets, and delete its keyspace
   (its `<dataplane><instance>` key prefix) whenever convenient.
   Note the member is *binary* (it usually contains NUL bytes), so shell command
   substitution will mangle it — issue the removal with a binary-safe client,
   e.g. `EVAL "return redis.call('ZREM', KEYS[1], string.char(0,1))" 1 <key>`
   for instance `(0, 1)`.

The keyspace deletion in step 4 is what the [`daw`](../services/daw) admin CLI is
for — `daw <pd> <redis> delete instance <dataplane> <instance>` summarises the
keys, and `--force` deletes them (it also clears the instance's Redis timestamp
entries). It understands the timbre key layout, so it deletes exactly the
namespaced range and nothing else.

Step 4's registry cleanup is manual by design — the registry has no TTL. A dead
instance left registered can be selected by the API and serve stale data.

## Single-node TiKV tips (compose deployments)

- The compose file caps TiKV's block cache (`configs/tikv/tikv.toml`) — by default
  TiKV takes ~45% of visible memory.
- PD's `evict-slow-store` scheduler is counterproductive with one store: if disk IO
  stalls under backfill load it "evicts leaders" with nowhere to send them, causing
  brief unavailability (the indexer's TiKV client treats this as fatal, and the
  container restart + work-unit re-execution handles it). Disable the scheduler
  once per cluster:

  ```bash
  docker compose exec pd ./pd-ctl -u http://localhost:2379 scheduler remove evict-slow-store-scheduler
  ```

## db-sync helper schema

The account, pool and epoch endpoint groups query the
[Koios](https://koios.rest) SQL layer (the `grest` schema) inside the db-sync
database. The compose `full` profile sets it up automatically:

- the one-shot `dbsync-migrations` service installs the schema from a pinned
  [koios-artifacts](https://github.com/cardano-community/koios-artifacts)
  release, populates `grest.genesis` from the node's genesis files, and applies
  the local overrides in `services/mapi/migrations/dataplane/`
- the `dbsync-caches` service refreshes the grest cached tables (active stake,
  epoch info, pool history, stake distribution) every ten minutes — the same
  jobs a Koios deployment runs via cron. Updates are skipped until db-sync
  reaches the chain tip, so cache-backed fields fill in shortly after sync
  completes
- the postgres image builds the
  [pg_bech32](https://github.com/cardano-community/pg_bech32) extension the
  Koios functions use to render bech32 identifiers
- cardano-db-sync runs with an explicit configuration
  (`configs/db-sync/<network>.json`) that enables the deduplicated `address`
  table the pool and account functions query — a stock db-sync database
  without this option does not work with these endpoint groups

When running against your own cardano-db-sync deployment, mirror those four
pieces: db-sync configured with `use_address_table`, pg_bech32 installed,
`run-dataplane-migrations.sh` applied once, and the cache refreshes scheduled.
Endpoints in these groups return errors until the schema exists and db-sync
has synced past the queried data.

## Monitoring

- **compressor**: `GET :50052/health` — upstream (node) slot vs pull/roll stage
  slots; alert when the gap grows. RocksDB internals for Prometheus on
  `:9090/metrics`.
- **polyphony**: no listener; watch logs and the freshness of its
  `tikv-timestamps:<dp>:<id>` entries in Redis.
- **mapi**: `GET :4000/healthcheck`; 5xx rates per endpoint.
- **tikv-gc**: alert when the run fails or when the safepoint stops advancing
  (retained MVCC versions grow unboundedly = disk grows unboundedly).
- **TiKV/PD**: standard TiKV monitoring (region count, store size, GC).
- **cardano-db-sync / Ogmios** (`full` profile): sync lag against the node —
  the endpoint groups they back go stale independently of the indexed data.

## Sizing notes

- **compressor rolldb**: stores the chain plus the TXO resolver; preview is a
  few GB, mainnet needs a few hundred GB.
- **TiKV**: grows with enabled reducers × MVCC retention. The GC cadence is your
  main disk lever.
- **cardano-node**: standard full-node requirements; no extra flags needed
  (compressor resolves inputs itself).

## Backups / recovery

- The rolldb and TiKV are both rebuildable from the chain — backups are a
  time-saver, not a correctness requirement. Snapshot volumes if resync time
  matters to you.
- A polyphony instance that got corrupted is simply replaced: new `instance_id`,
  backfill, swap over — same flow as an upgrade.
- A corrupted rolldb is resynced from the node (or restored from a volume
  snapshot); indexer instances resume from their cursors once compressor is
  back at the tip.
