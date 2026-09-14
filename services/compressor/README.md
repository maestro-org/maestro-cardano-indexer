# compressor

A Cardano **chain follower and enrichment engine**: the first stage of the
indexing pipeline. It connects directly to a cardano-node over the node socket
(Ouroboros chainsync, node-to-client, via
[Pallas](https://github.com/txpipe/pallas)), stores blocks in a local RocksDB
database (the *rolldb*), and precomputes what downstream indexers need but
can't get from raw blocks alone:

- **Resolved inputs**: every transaction input is resolved to the full previous
  output it spends (raw CBOR plus the slot it was created at), so consumers
  never need their own UTxO index.
- **Era tags**: each resolved output carries the era needed to decode it, so a
  consumer can handle a Conway transaction spending a Byron-era output without
  any era bookkeeping of its own.
- **Genesis UTxOs**: the Byron genesis UTxOs are seeded from the genesis JSON
  (the `[byron]` config entry) at first start, so input resolution is complete
  from slot zero.

The result — blocks bundled with their *resolver context* — is served over gRPC
(see [`proto/sync/v1/sync.proto`](../../proto/sync/v1/sync.proto), package
`compressor.sync.v1`) to any number of downstream indexer instances:

| RPC | Purpose |
|---|---|
| `PageBlocksWithContext` | Bulk paged download of historical blocks (initial sync) |
| `StreamUpdatesWithContext` | Apply/undo/reset stream at the chain tip (rollback-aware) |

Because inputs are resolved **once**, adding another indexer instance costs no
extra node or UTxO-resolution work — this is what makes running many parallel
indexer instances cheap.

There is no mempool surface: compressor serves committed chain state only.

## Running

```
compressor <config.toml> <subcommand>
```

| Subcommand | Meaning |
|---|---|
| `sync` | Sync the rolldb from the node only |
| `serve` | Serve gRPC from an existing rolldb only |
| `daemon` | `sync` + `serve` (recommended) |

## Configuration

Layered: `compressor.toml` in the working directory, then the explicit config
file argument, then `COMPR`-prefixed environment variables. See
[`configs/compressor/`](../../configs/compressor) for complete examples.

| Key | Meaning |
|---|---|
| `logging.max_level` | Log level (`info`, `debug`, …) |
| `chain_db.path` | RocksDB rolldb directory |
| `chain_db.immutable_after_slots` | Depth of the mutable window in slots: blocks deeper than this are stored immutably (rollback log kept above it). Also caps how far behind the tip an indexer can intersect the mutable stream, so it bounds the indexer's effective rollback runway |
| `sync.node_socket` | Path to the cardano-node socket |
| `sync.network_magic` | Network magic: preview = 2, preprod = 1, mainnet = 764824073 |
| `sync.health_endpoint` | HTTP health listener (`/health` reports sync progress) |
| `serve.listen_address` | gRPC listener (default port 50051) |
| `byron.path` | Byron genesis JSON for the network — mainnet/preprod/preview files ship in [`genesis/`](genesis) |

## Ports

- `50051` — gRPC sync service
- `50052` — HTTP health (`curl localhost:50052/health` — upstream node slot vs
  pull/roll stage slots)
- `9090` — Prometheus metrics (`/metrics`: RocksDB gauges; plus raw RocksDB
  property dumps at `/stats`, `/cfstats`, `/dbstats`, `/levelstats`,
  `/sstables`, `/options_statistics`)
