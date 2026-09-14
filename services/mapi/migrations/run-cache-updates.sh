#!/bin/sh
# Periodically refreshes the grest cached tables backing the account, pool
# and epoch endpoint groups (the same jobs Koios runs via cron). Each update
# is skipped while db-sync is still catching up (no new block in the last
# five minutes), mirroring the upstream cron scripts.

set -u

INTERVAL_SECONDS="${INTERVAL_SECONDS:-600}"

while true; do
    tip=$(psql -qbt -c "SELECT EXTRACT(EPOCH FROM time)::integer FROM block ORDER BY id DESC LIMIT 1;" 2>/dev/null | xargs)

    if [ -n "$tip" ] && [ $(($(date +%s) - tip)) -le 300 ]; then
        echo "running grest cache updates..."
        psql -qbt -c "SELECT grest.epoch_info_cache_update();" >/dev/null 2>&1 || echo "epoch_info_cache_update failed"
        psql -qbt -c "SELECT grest.active_stake_cache_update_check();" >/dev/null 2>&1 || echo "active_stake_cache_update_check failed"
        psql -qbt -c "SELECT grest.stake_distribution_cache_update_check();" >/dev/null 2>&1 || echo "stake_distribution_cache_update_check failed"
        psql -qbt -c "SELECT grest.pool_history_cache_update();" >/dev/null 2>&1 || echo "pool_history_cache_update failed"
        echo "cache updates done"
    else
        echo "db-sync not at tip yet, skipping cache updates"
    fi

    sleep "$INTERVAL_SECONDS"
done
