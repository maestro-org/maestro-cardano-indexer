#!/bin/sh
# Sets up the db-sync helper schema (grest) used by the account, pool and
# epoch endpoint groups:
#
#   1. installs the Koios SQL layer (schema, helper functions, cached tables
#      and RPC functions) from a pinned koios-artifacts release
#   2. populates grest.genesis from the node's genesis files
#   3. applies the local overrides in ./dataplane
#
# Requires network access (downloads the koios-artifacts release tarball) and
# the cardano-node config volume mounted at /config. Safe to re-run.

set -e

# byte-order file sorting, so e.g. pool_delegators.sql applies before
# pool_delegators_history.sql regardless of locale collation
export LC_ALL=C

KOIOS_VERSION="v1.3.0"
MIGRATIONS_DIR="$(dirname "$0")/dataplane"
NETWORK="${NETWORK:-preprod}"
CONFIG_DIR="/config/${NETWORK}"

echo "installing prerequisites..."
apt-get update -qq && apt-get install -y -qq curl jq >/dev/null

# db-sync creates its schema on first start; the grest functions reference
# its tables, so wait until the base tables exist
echo "waiting for the db-sync schema..."
until psql -qtA -c "SELECT 1 FROM information_schema.tables WHERE table_name = 'block';" 2>/dev/null | grep -q 1; do
    sleep 10
done

echo "fetching koios-artifacts ${KOIOS_VERSION}..."
curl -sfL "https://github.com/cardano-community/koios-artifacts/archive/refs/tags/${KOIOS_VERSION}.tar.gz" -o /tmp/koios.tar.gz
mkdir -p /tmp/koios && tar -xzf /tmp/koios.tar.gz -C /tmp/koios --strip-components=1
GREST_DIR=/tmp/koios/files/grest

echo "applying grest base schema..."
psql --variable=ON_ERROR_STOP=1 -c "CREATE EXTENSION IF NOT EXISTS pg_bech32;" >/dev/null
psql --single-transaction --variable=ON_ERROR_STOP=1 --file "${GREST_DIR}/rpc/db-scripts/basics.sql" >/dev/null

echo "populating grest.genesis from ${CONFIG_DIR}..."
SHELLEY="${CONFIG_DIR}/shelley-genesis.json"
ALONZO="${CONFIG_DIR}/alonzo-genesis.json"
psql --variable=ON_ERROR_STOP=1 >/dev/null <<EOF
TRUNCATE grest.genesis;
INSERT INTO grest.genesis VALUES (
  '$(jq -r .networkMagic "$SHELLEY")',
  '$(jq -r .networkId "$SHELLEY")',
  '$(jq -r .activeSlotsCoeff "$SHELLEY")',
  '$(jq -r .updateQuorum "$SHELLEY")',
  '$(jq -r .maxLovelaceSupply "$SHELLEY")',
  '$(jq -r .epochLength "$SHELLEY")',
  '$(jq -r .systemStart "$SHELLEY")',
  '$(jq -r .slotsPerKESPeriod "$SHELLEY")',
  '$(jq -r .slotLength "$SHELLEY")',
  '$(jq -r .maxKESEvolutions "$SHELLEY")',
  '$(jq -r .securityParam "$SHELLEY")',
  '$(jq -c . "$ALONZO" | sed "s/'/''/g")'
);
EOF

# Function directories in dependency order (utilities and cached tables
# first), then the endpoint groups the API uses.
for dir in 000_utilities 00_blockchain 01_cached_tables account address assets blocks epoch pool script transactions; do
    for file in $(find "${GREST_DIR}/rpc/${dir}" -type f -name '*.sql' | sort); do
        echo "applying $(basename "$file")..."
        psql --single-transaction --variable=ON_ERROR_STOP=1 --file "$file" >/dev/null
    done
done

echo "applying local overrides..."
for file in $(find "${MIGRATIONS_DIR}" -type f -name '*.sql' | sort); do
    echo "applying $(basename "$file")..."
    psql --single-transaction --variable=ON_ERROR_STOP=1 --file "$file" >/dev/null
done

echo "db-sync helper schema ready"
