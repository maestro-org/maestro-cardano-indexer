# timbre

The **TiKV key/value encoding schema** — a pure library crate (no I/O) defining
the byte-level layout of every key and value the indexer writes and the API
reads. It is the contract between [polyphony](../../services/polyphony)
(writer) and [mapi](../../services/mapi) (reader): both depend on this crate,
so the schema can never drift between them.

The building blocks are a `KeyEncoder` — constructed from a writing instance's
identity, which it prefixes onto every key — plus free encode/decode functions
per reducer (`src/encoding/encode.rs`, `src/encoding/decode.rs`, and the
per-reducer modules alongside them).

## The key shape

Every key starts with the writing instance's identity and a **plane tag**
separating the kinds of data sharing the store:

```
<dataplane_id: u8><instance_id: u8><plane tag><...>
```

| Plane tag | Contents |
|---|---|
| `C` | The instance's cursor (its committed chain position) |
| `D` | Indexed data — one reducer tag byte follows, then the reducer's key body |
| `R` | Rollback metadata (persisted inverse actions for recent blocks) |
| `I` | Instance info record |
| `X` | Index plane (declared; unused by current reducers) |
| `T` | Token-registry metadata (fed by an external ingestor, not a reducer) |

The 2-byte identity prefix is what lets many indexer instances share one TiKV
cluster without touching each other's keyspace — and makes retiring an
instance a single contiguous range delete (`Namespace` in
`src/encoding/namespace.rs`).

Two rules make key bodies scannable. Integers encode **big-endian**, so
lexicographic byte order equals numeric order — a byte-range scan *is* a
slot-range scan, and slot-ordered keys serve "history of X" queries directly.
And **field order is the query plan**: each key orders its fields to match its
endpoint's access pattern (e.g. address first for a per-address prefix scan,
slot next for range bounds, outpoint last to disambiguate).

The `BREAK` delimiter (0x60, `` ` ``) separates variable-length fields, and is
also how prefix scans are bounded: the range
`[<prefix><BREAK> .. <prefix><BREAK+1>)` covers exactly the keys under a
prefix, with the exclusive end obtained by incrementing the delimiter byte.
For prefixes without a trailing delimiter, `prefix_key_range` increments the
prefix's last non-`0xFF` byte instead.

Pagination cursors are (suffixes of) the last key returned, base64url-encoded
and handed to API clients as opaque `next_cursor` tokens — resuming a scan is
seeking to a key: no offsets, no skip cost, stable under concurrent writes.

## Reducer tags

Under the `D` plane, one byte names the reducer that owns the key
(`src/encoding/encode.rs`):

| Tag | Reducer | Key body (after the tag) |
|---|---|---|
| `a` | TxByHash | `tx_hash(32)` |
| `b` | TxCountByAddress | `address-with-type` |
| `c` | HoldersByAsset | `policy` `BREAK` `asset_name` `BREAK` `address-with-type` — declared; not written by any current reducer |
| `d` | UtxosByPolicy | `policy` `BREAK` `slot(u64)` `utxo_hash(32)` `utxo_index(u64)` |
| `e` | UtxosByAsset | `policy` `BREAK` `asset_name` `BREAK` `slot(u64)` `utxo_hash(32)` `utxo_index(u64)` |
| `f` | LovelaceByAddress (the `BalanceByAddress` reducer) | `address-with-type` |
| `g` | DatumByHash | `datum_hash(32)` |
| `h` | BlockByHeight | `'0'` `height(u64)` (block by height) / `'1'` `block_hash(32)` (height by hash) |
| `i` | TxsByAddress | `address-with-type` `BREAK` `slot(u64)` `block_index(u16)` `tx_hash(32)` |
| `j` | TxsByPayCred | `payment-cred` `BREAK` `slot(u64)` `block_index(u16)` `tx_hash(32)` |
| `k`, `l`, `m` | — | Permanently reserved: used by retired reducers; reusing them would make historical data ambiguous |
| `n` | SupplyByAsset | `policy` `BREAK` `asset_name` |
| `o` | ScriptByHash | `script_hash(28)` |
| `p` | TxsByPolicy | `policy` `BREAK` `slot(u64)` `block_index(u16)` |
| `q` | UpdatesByPolicy | `policy` `BREAK` `slot(u64)` `block_index(u16)` |
| `r` | MintMetadataByAsset | `policy` `asset_name(short-bytestring)` `BREAK` `slot(u64)` `block_index(u16)` |
| `s` | Cip25MetadataByAsset | `policy` `asset_name(short-bytestring)` |
| `t` | BlockByTx | `tx_hash(32)` |
| `y` | UtxosByShelleyAddress (the `UtxoCborByAddress` reducer) | `payment-cred` `staking-cred` `BREAK` `slot(u64)` `utxo_hash(32)` `utxo_index(u64)` |
| `z` | UtxosByByronAddress (the `UtxoCborByAddress` reducer) | `byron-address-or-hash` `BREAK` `slot(u64)` `utxo_hash(32)` `utxo_index(u64)` |

`address-with-type` is a one-byte Shelley/Byron discriminator followed by the
encoded address; Byron addresses longer than the size cap are stored as their
Blake2b hash with a marker byte. `asset_name(short-bytestring)` is a one-byte
length prefix followed by the (up to 32-byte) asset name.
