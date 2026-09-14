# Reducers

A reducer is one query pattern, indexed. Each one consumes enriched blocks and
emits key/value actions; enabling one is a single `[[reducers]]` entry in the
polyphony config. Because compressor resolves inputs (with their era tags),
most reducers are short — they read facts off the enriched block rather than
computing them.

## Catalogue

### Chain / transactions

| Reducer | Answers |
|---|---|
| `BlockByHeight` | Block summary by height, and height by block hash (also the reducer behind `/chain-tip` and the API healthcheck) |
| `BlockByTx` | Which block is this tx in? |
| `TxByHash` | Raw transaction CBOR by hash — decoded on read for `/transactions/{tx_hash}` |
| `TxsByAddress` | Transaction history of an address |
| `TxsByPayCred` | Transaction history of a payment credential |
| `TxCountByAddress` | How many transactions touched this address? |

### Address / balance

| Reducer | Answers |
|---|---|
| `UtxoCborByAddress` | Current UTxOs of an address, with raw output CBOR (Shelley and Byron addresses) |
| `BalanceByAddress` | Lovelace balance of an address / payment credential |

### Assets / policies

| Reducer | Answers |
|---|---|
| `SupplyByAsset` | Current supply of a native asset |
| `UtxosByAsset` | UTxOs holding an asset (also drives holder queries) |
| `UtxosByPolicy` | UTxOs holding any asset of a policy |
| `TxsByPolicy` | Transactions involving a policy's assets |
| `UpdatesByPolicy` | Mint/burn activity of a policy over time |
| `MintMetadataByAsset` | An asset's minting transactions and attached metadata |
| `Cip25MetadataByAsset` | CIP-25 (NFT) metadata by asset |

### Scripts / datums

| Reducer | Answers |
|---|---|
| `ScriptByHash` | Script bytes (and kind) by script hash |
| `DatumByHash` | Datum by hash — also resolves inline/witness datums for tx decoding |

Five of the asset/policy reducers (`Cip25MetadataByAsset`,
`MintMetadataByAsset`, `ScriptByHash`, `SupplyByAsset`, `UpdatesByPolicy`) are
advertised in the instance registry under one grouped name,
`assets-policies` — the endpoints that read them cross those reducers freely,
so they are deployed and swapped over as a unit
(`reducers::Config::instance_name` in
`services/polyphony/src/reducers/mod.rs`).

The token-registry keyspace served by `/assets` metadata fields is **not** a
reducer: it is fed by an external ingestor under its own key plane and
addressed by IDs from mapi configuration (see the
[mapi README](../services/mapi/README.md)).

## Writing a reducer

1. **Define the key layout** in `crates/timbre/src/encoding/`: pick an unused
   tag byte (see the table in the [timbre README](../crates/timbre/README.md)),
   design key bytes so that range scans return results in the order your
   queries want, and add the encode/decode functions (`encode.rs` /
   `decode.rs`, or a dedicated module like the existing per-reducer files).
2. **Implement the reducer** in `services/polyphony/src/reducers/<name>.rs`: a
   `Config` struct (often empty) and a `reduce_block` producing reducer outputs
   from the enriched block. Register it in the `Config` enum in
   `reducers/mod.rs` — including `instance_name()`, the name used in the Redis
   instance registry — and map its output to storage actions in
   `services/polyphony/src/storage/tikv.rs`.
3. **Read it back** in `services/mapi`: add the reducer to the `ReducerType` /
   `InstanceType` enums in `src/key_resolver.rs` (the instance name must match
   polyphony's exactly) and build endpoints on snapshots from
   `PolyphonyWrapper`.
4. **Backfill it live** — this is the point of the architecture: run a new
   polyphony instance with the new reducer under a fresh `instance_id`; it
   backfills in parallel and enters service automatically when it reaches the
   tip ([design.md](design.md#1-fixing-a-broken-indexer-with-zero-downtime)).

Reducers must be **deterministic and invertible** — the storage stage records
inverse actions per block so rollbacks unwind exactly. Stick to
set/delete/increment actions and derive everything from the block + enrichment
context (never from wall-clock or external I/O).
