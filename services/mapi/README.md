# mapi

The **HTTP API layer**: a stateless axum server that reads pre-indexed data out
of TiKV and serves it as a documented REST API (~60 endpoints). It performs no
indexing of its own — the chain data is written by
[polyphony](../polyphony) instances, and mapi's job is to (1) discover which
instance to read from, (2) decode timbre keys/values into API responses, and
(3) delegate the endpoint groups that need ledger-derived state to auxiliary
backends.

## How reads work

For each TiKV-backed request, mapi:

1. **Resolves an instance** — looks up the per-reducer instance registry in
   Redis (`cardano:<network>:<name>:scores` sorted sets) to find the
   `(dataplane_id, instance_id)` currently serving each reducer the endpoint
   needs. Which instance serves is data, not configuration, so indexer
   swap-overs need no API restart.
2. **Opens a snapshot** — a TiKV snapshot at the current timestamp, and scans
   the resolved instance's keyspace.
3. **Stamps the response** — `last_updated` is the block the serving instance
   had indexed to, read from that instance's own cursor key in the same
   snapshot as the data, so the stamp and the data always agree.

Reads are served at the current tip of the serving instance; unlike the
Bitcoin sibling stack, there is no cross-reducer snapshot unification and no
time-travel querying.

## Backends

| Backend | Endpoint groups |
|---|---|
| **TiKV** (via the Redis registry) | blocks, transactions, addresses, assets, policies, datums, scripts, `/chain-tip`, `/ecosystem/adahandle` |
| **cardano-db-sync** (Postgres) | accounts, pools, epochs, `/era-summaries`, `/protocol-parameters` (partially), `/system-start`, pool metadata |
| **Ogmios v6** (websocket) | current protocol parameters and the Plutus cost models behind `/transactions/evaluate` (cached with a short TTL and single-flight refresh) |

`/transactions/evaluate` runs phase-two Plutus evaluation **in-process** via
[uplc](https://github.com/aiken-lang/aiken), resolving the transaction's inputs
from the indexed keyspace and taking cost models from Ogmios; it returns
execution units per redeemer. Transaction *submission* is out of scope — the
API is read-only, and submitting Cardano transactions is a separate concern
(the node, Ogmios, or a dedicated submission service).

`/ecosystem/adahandle/{handle}` resolves ADA Handles (both the CIP-68 and the
original standard) against the public handle policy id, using the indexed
asset-UTxO data.

The **token registry** metadata returned on asset endpoints comes from a
keyspace fed by an external ingestor, not a reducer; it is addressed by the
optional `TOKEN_REGISTRY_*` variables below, and when they are unset the API
serves `null` token-registry metadata.

## Running

```
mapi server        # start the API
mapi docs          # regenerate docs/indexer/swagger.json and exit
```

The OpenAPI document is committed at
[`docs/indexer/swagger.json`](docs/indexer/swagger.json); no Swagger UI is
served by the binary — point any OpenAPI viewer at the JSON.

Configuration is environment-only:

| Env | Default | Meaning |
|---|---|---|
| `NETWORK` | `mainnet` | `mainnet`, `preprod`, or `preview` |
| `MAPI_PORT` | `4000` | HTTP listener port (binds `0.0.0.0`) |
| `TIKV_PD_CLIENT` | `127.0.0.1:2379` | TiKV PD endpoint |
| `TIKV_MAX_CONNECTIONS` | `5` | TiKV connection pool size |
| `REDIS` | required | Redis URL (instance registry) |
| `DBSYNC_DB_URL` | `postgres://cexplorer@127.0.0.1` | cardano-db-sync Postgres |
| `DBSYNC_MAX_CONNECTIONS` | `5` | Postgres pool size |
| `OGMIOS_V6_URL` | `ws://127.0.0.1:1338` | Ogmios v6 websocket |
| `SCAN_TIMEOUT_MILLIS` | `5000` | Per-request TiKV scan budget |
| `OGMIOS_CACHE_TTL_SECS` | `60` | TTL for cached Ogmios/db-sync responses (must be > 0) |
| `OGMIOS_QUERY_TIMEOUT_MILLIS` | `10000` | Per-attempt Ogmios query timeout (must be > 0) |
| `OGMIOS_MAX_RETRIES` | `2` | Ogmios query retries (max 10) |
| `TOKEN_REGISTRY_DATAPLANE_ID` | unset | Dataplane id of the token-registry keyspace |
| `TOKEN_REGISTRY_INSTANCE_ID` | unset | Instance id of the token-registry keyspace |

In the compose stack the db-sync and Ogmios endpoints point at the `full`
profile's services; without that profile those endpoint groups return errors
and the rest of the API works.

## Security note

mapi performs **no authentication or rate limiting in-process** — deploy it
behind a gateway if you expose it publicly.
