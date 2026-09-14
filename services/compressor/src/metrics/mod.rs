use std::sync::Arc;

use actix_web::{
    web::{self},
    App, HttpResponse, HttpServer, Responder,
};
use prometheus::{register_int_gauge, Encoder, IntGauge, TextEncoder};
use rocksdb::TransactionDB;
use tracing::info;

use crate::storage::{options, ChainDB};
struct Metrics {
    db: Arc<TransactionDB>,
    // Metrics:
    num_files_at_level_0: IntGauge,
    compression_ratio_at_level_0: IntGauge,
    cur_size_active_mem_table: IntGauge,
    estimate_num_keys: IntGauge,
    block_cache_usage: IntGauge,
    num_running_flushes: IntGauge,
    num_running_compactions: IntGauge,
    background_errors: IntGauge,
    estimate_live_data_size: IntGauge,
    mem_table_flush_pending: IntGauge,
    num_snapshots: IntGauge,
    oldest_snapshot_time: IntGauge,
    num_live_versions: IntGauge,
    total_sst_files_size: IntGauge,
    live_sst_files_size: IntGauge,
    estimate_pending_compaction_bytes: IntGauge,
    block_cache_pinned_usage: IntGauge,
    cur_size_all_mem_tables: IntGauge,
    size_all_mem_tables: IntGauge,
    num_entries_active_mem_table: IntGauge,
    num_entries_imm_mem_tables: IntGauge,
    num_deletes_active_mem_table: IntGauge,
    num_deletes_imm_mem_tables: IntGauge,
    estimate_table_readers_mem: IntGauge,
    is_file_deletions_enabled: IntGauge,
    estimate_oldest_key_time: IntGauge,
    actual_delayed_write_rate: IntGauge,
    is_write_stopped: IntGauge,
    num_immutable_mem_table: IntGauge,
    num_immutable_mem_table_flushed: IntGauge,
    compaction_pending: IntGauge,
    current_super_version_number: IntGauge,
    min_log_number_to_keep: IntGauge,
    min_obsolete_sst_number_to_keep: IntGauge,
    base_level: IntGauge,
    block_cache_capacity: IntGauge,
    approximate_mem_table_total: IntGauge,
    approximate_mem_table_unflushed: IntGauge,
    approximate_mem_table_readers_total: IntGauge,
    approximate_cache_total: IntGauge,
}

impl Metrics {
    fn new(chain_db: &ChainDB) -> Self {
        let num_files_at_level_0 = register_int_gauge!(
            "rocksdb_num_files_at_level_0",
            "Number of files at level 0."
        )
        .unwrap();

        let compression_ratio_at_level_0 = register_int_gauge!(
            "rocksdb_compression_ratio_at_level_0",
            "Compression ratio at level 0."
        )
        .unwrap();

        let cur_size_active_mem_table = register_int_gauge!(
            "rocksdb_cur_size_active_mem_table",
            "Approximate size of active memtable (bytes)."
        )
        .unwrap();

        let estimate_num_keys = register_int_gauge!(
            "rocksdb_estimate_num_keys",
            "Estimated number of total keys."
        )
        .unwrap();

        let block_cache_usage = register_int_gauge!(
            "rocksdb_block_cache_usage",
            "Memory size for the entries residing in block cache."
        )
        .unwrap();

        let num_running_flushes = register_int_gauge!(
            "rocksdb_num_running_flushes",
            "Number of currently running flushes."
        )
        .unwrap();

        let num_running_compactions = register_int_gauge!(
            "rocksdb_num_running_compactions",
            "Number of currently running compactions."
        )
        .unwrap();

        let background_errors = register_int_gauge!(
            "rocksdb_background_errors",
            "Accumulated number of background errors."
        )
        .unwrap();

        let estimate_live_data_size = register_int_gauge!(
            "rocksdb_estimate_live_data_size",
            "Estimate of the amount of live data in bytes."
        )
        .unwrap();

        let mem_table_flush_pending = register_int_gauge!(
            "rocksdb_mem_table_flush_pending",
            "Indicates if a memtable flush is pending."
        )
        .unwrap();

        let num_snapshots = register_int_gauge!(
            "rocksdb_num_snapshots",
            "Number of unreleased snapshots of the database."
        )
        .unwrap();

        let oldest_snapshot_time = register_int_gauge!(
            "rocksdb_oldest_snapshot_time",
            "Unix timestamp of oldest unreleased snapshot."
        )
        .unwrap();

        let num_live_versions =
            register_int_gauge!("rocksdb_num_live_versions", "Number of live versions.").unwrap();

        let total_sst_files_size = register_int_gauge!(
            "rocksdb_total_sst_files_size",
            "Total size of all SST files."
        )
        .unwrap();

        let live_sst_files_size = register_int_gauge!(
            "rocksdb_live_sst_files_size",
            "Total size of all SST files belonging to the latest LSM tree."
        )
        .unwrap();

        let estimate_pending_compaction_bytes = register_int_gauge!(
            "rocksdb_estimate_pending_compaction_bytes",
            "Estimated total number of bytes compaction needs to rewrite."
        )
        .unwrap();

        let block_cache_pinned_usage = register_int_gauge!(
            "rocksdb_block_cache_pinned_usage",
            "Memory size for the entries being pinned."
        )
        .unwrap();

        let cur_size_all_mem_tables = register_int_gauge!(
            "rocksdb_cur_size_all_mem_tables",
            "Approximate size of active and unflushed immutable memtables (bytes)."
        )
        .unwrap();

        let size_all_mem_tables = register_int_gauge!(
            "rocksdb_size_all_mem_tables",
            "Approximate size of active, unflushed immutable, and pinned immutable memtables (bytes)."
        )
        .unwrap();

        let num_entries_active_mem_table = register_int_gauge!(
            "rocksdb_num_entries_active_mem_table",
            "Total number of entries in the active memtable."
        )
        .unwrap();

        let num_entries_imm_mem_tables = register_int_gauge!(
            "rocksdb_num_entries_imm_mem_tables",
            "Total number of entries in the unflushed immutable memtables."
        )
        .unwrap();

        let num_deletes_active_mem_table = register_int_gauge!(
            "rocksdb_num_deletes_active_mem_table",
            "Total number of delete entries in the active memtable."
        )
        .unwrap();

        let num_deletes_imm_mem_tables = register_int_gauge!(
            "rocksdb_num_deletes_imm_mem_tables",
            "Total number of delete entries in the unflushed immutable memtables."
        )
        .unwrap();

        let estimate_table_readers_mem = register_int_gauge!(
            "rocksdb_estimate_table_readers_mem",
            "Estimated memory used for reading SST tables, excluding memory used in block cache."
        )
        .unwrap();

        let is_file_deletions_enabled = register_int_gauge!(
            "rocksdb_is_file_deletions_enabled",
            "Indicates if deletion of obsolete files is enabled."
        )
        .unwrap();

        let estimate_oldest_key_time = register_int_gauge!(
            "rocksdb_estimate_oldest_key_time",
            "Estimation of oldest key timestamp in the DB."
        )
        .unwrap();

        let actual_delayed_write_rate = register_int_gauge!(
            "rocksdb_actual_delayed_write_rate",
            "Current actual delayed write rate. 0 means no delay."
        )
        .unwrap();

        let is_write_stopped = register_int_gauge!(
            "rocksdb_is_write_stopped",
            "Indicates if write has been stopped."
        )
        .unwrap();

        let num_immutable_mem_table = register_int_gauge!(
            "rocksdb_num_immutable_mem_table",
            "Number of immutable memtables that have not yet been flushed."
        )
        .unwrap();

        let num_immutable_mem_table_flushed = register_int_gauge!(
            "rocksdb_num_immutable_mem_table_flushed",
            "Number of immutable memtables that have already been flushed."
        )
        .unwrap();

        let compaction_pending = register_int_gauge!(
            "rocksdb_compaction_pending",
            "Returns 1 if at least one compaction is pending; otherwise, returns 0."
        )
        .unwrap();

        let current_super_version_number = register_int_gauge!(
            "rocksdb_current_super_version_number",
            "Number of current LSM version."
        )
        .unwrap();

        let min_log_number_to_keep = register_int_gauge!(
            "rocksdb_min_log_number_to_keep",
            "Returns the minimum log number of the log files that should be kept."
        )
        .unwrap();

        let min_obsolete_sst_number_to_keep = register_int_gauge!(
            "rocksdb_min_obsolete_sst_number_to_keep",
            "Returns the minimum file number for an obsolete SST to be kept."
        )
        .unwrap();

        let base_level = register_int_gauge!(
            "rocksdb_base_level",
            "Returns number of level to which L0 data will be compacted."
        )
        .unwrap();

        let block_cache_capacity = register_int_gauge!(
            "rocksdb_block_cache_capacity",
            "Returns block cache capacity."
        )
        .unwrap();

        let approximate_mem_table_total = register_int_gauge!(
            "rocksdb_memory_usage_approximate_mem_table_total",
            "RocksDB memory usage: Approximate memory usage of all the mem-tables"
        )
        .unwrap();

        let approximate_mem_table_unflushed = register_int_gauge!(
            "rocksdb_memory_usage_approximate_mem_table_unflushed",
            "RocksDB memory usage: Approximate memory usage of un-flushed mem-tables"
        )
        .unwrap();

        let approximate_mem_table_readers_total = register_int_gauge!(
            "rocksdb_memory_usage_approximate_mem_table_readers_total",
            "RocksDB memory usage: Approximate memory usage of all the table readers"
        )
        .unwrap();

        let approximate_cache_total = register_int_gauge!(
            "rocksdb_memory_usage_approximate_cache_total",
            "RocksDB memory usage: Approximate memory usage by cache"
        )
        .unwrap();

        let db = chain_db.db.clone();
        Metrics {
            db,
            num_files_at_level_0,
            compression_ratio_at_level_0,
            cur_size_active_mem_table,
            estimate_num_keys,
            block_cache_usage,
            num_running_flushes,
            num_running_compactions,
            background_errors,
            estimate_live_data_size,
            mem_table_flush_pending,
            num_snapshots,
            oldest_snapshot_time,
            num_live_versions,
            total_sst_files_size,
            live_sst_files_size,
            estimate_pending_compaction_bytes,
            block_cache_pinned_usage,
            cur_size_all_mem_tables,
            size_all_mem_tables,
            num_entries_active_mem_table,
            num_entries_imm_mem_tables,
            num_deletes_active_mem_table,
            num_deletes_imm_mem_tables,
            estimate_table_readers_mem,
            is_file_deletions_enabled,
            estimate_oldest_key_time,
            actual_delayed_write_rate,
            is_write_stopped,
            num_immutable_mem_table,
            num_immutable_mem_table_flushed,
            compaction_pending,
            current_super_version_number,
            min_log_number_to_keep,
            min_obsolete_sst_number_to_keep,
            base_level,
            block_cache_capacity,
            approximate_mem_table_total,
            approximate_mem_table_unflushed,
            approximate_mem_table_readers_total,
            approximate_cache_total,
        }
    }

    fn update_metrics(&self) {
        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::num_files_at_level(0))
        {
            if let Ok(num_files) = value.parse::<i64>() {
                self.num_files_at_level_0.set(num_files);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::compression_ratio_at_level(0))
        {
            if let Ok(ratio) = value.parse::<f64>() {
                self.compression_ratio_at_level_0.set(ratio as i64);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::CUR_SIZE_ACTIVE_MEM_TABLE)
        {
            if let Ok(size) = value.parse::<i64>() {
                self.cur_size_active_mem_table.set(size);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::ESTIMATE_NUM_KEYS)
        {
            if let Ok(num_keys) = value.parse::<i64>() {
                self.estimate_num_keys.set(num_keys);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::BLOCK_CACHE_USAGE)
        {
            if let Ok(usage) = value.parse::<i64>() {
                self.block_cache_usage.set(usage);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::NUM_RUNNING_FLUSHES)
        {
            if let Ok(num_flushes) = value.parse::<i64>() {
                self.num_running_flushes.set(num_flushes);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::NUM_RUNNING_COMPACTIONS)
        {
            if let Ok(num_compactions) = value.parse::<i64>() {
                self.num_running_compactions.set(num_compactions);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::BACKGROUND_ERRORS)
        {
            if let Ok(errors) = value.parse::<i64>() {
                self.background_errors.set(errors);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::ESTIMATE_LIVE_DATA_SIZE)
        {
            if let Ok(size) = value.parse::<i64>() {
                self.estimate_live_data_size.set(size);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::MEM_TABLE_FLUSH_PENDING)
        {
            if let Ok(pending) = value.parse::<i64>() {
                self.mem_table_flush_pending.set(pending);
            }
        }

        if let Ok(Some(value)) = self.db.property_value(rocksdb::properties::NUM_SNAPSHOTS) {
            if let Ok(snapshots) = value.parse::<i64>() {
                self.num_snapshots.set(snapshots);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::OLDEST_SNAPSHOT_TIME)
        {
            if let Ok(time) = value.parse::<i64>() {
                self.oldest_snapshot_time.set(time);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::NUM_LIVE_VERSIONS)
        {
            if let Ok(versions) = value.parse::<i64>() {
                self.num_live_versions.set(versions);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::TOTAL_SST_FILES_SIZE)
        {
            if let Ok(size) = value.parse::<i64>() {
                self.total_sst_files_size.set(size);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::LIVE_SST_FILES_SIZE)
        {
            if let Ok(size) = value.parse::<i64>() {
                self.live_sst_files_size.set(size);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::ESTIMATE_PENDING_COMPACTION_BYTES)
        {
            if let Ok(bytes) = value.parse::<i64>() {
                self.estimate_pending_compaction_bytes.set(bytes);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::BLOCK_CACHE_PINNED_USAGE)
        {
            if let Ok(usage) = value.parse::<i64>() {
                self.block_cache_pinned_usage.set(usage);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::CUR_SIZE_ALL_MEM_TABLES)
        {
            if let Ok(size) = value.parse::<i64>() {
                self.cur_size_all_mem_tables.set(size);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::SIZE_ALL_MEM_TABLES)
        {
            if let Ok(size) = value.parse::<i64>() {
                self.size_all_mem_tables.set(size);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::NUM_ENTRIES_ACTIVE_MEM_TABLE)
        {
            if let Ok(num_entries) = value.parse::<i64>() {
                self.num_entries_active_mem_table.set(num_entries);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::NUM_ENTRIES_IMM_MEM_TABLES)
        {
            if let Ok(num_entries) = value.parse::<i64>() {
                self.num_entries_imm_mem_tables.set(num_entries);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::NUM_DELETES_ACTIVE_MEM_TABLE)
        {
            if let Ok(num_deletes) = value.parse::<i64>() {
                self.num_deletes_active_mem_table.set(num_deletes);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::NUM_DELETES_IMM_MEM_TABLES)
        {
            if let Ok(num_deletes) = value.parse::<i64>() {
                self.num_deletes_imm_mem_tables.set(num_deletes);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::ESTIMATE_TABLE_READERS_MEM)
        {
            if let Ok(mem) = value.parse::<i64>() {
                self.estimate_table_readers_mem.set(mem);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::IS_FILE_DELETIONS_ENABLED)
        {
            if let Ok(enabled) = value.parse::<i64>() {
                self.is_file_deletions_enabled.set(enabled);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::ESTIMATE_OLDEST_KEY_TIME)
        {
            if let Ok(time) = value.parse::<i64>() {
                self.estimate_oldest_key_time.set(time);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::ACTUAL_DELAYED_WRITE_RATE)
        {
            if let Ok(rate) = value.parse::<i64>() {
                self.actual_delayed_write_rate.set(rate);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::IS_WRITE_STOPPED)
        {
            if let Ok(stopped) = value.parse::<i64>() {
                self.is_write_stopped.set(stopped);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::NUM_IMMUTABLE_MEM_TABLE)
        {
            if let Ok(num) = value.parse::<i64>() {
                self.num_immutable_mem_table.set(num);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::NUM_IMMUTABLE_MEM_TABLE_FLUSHED)
        {
            if let Ok(num) = value.parse::<i64>() {
                self.num_immutable_mem_table_flushed.set(num);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::COMPACTION_PENDING)
        {
            if let Ok(pending) = value.parse::<i64>() {
                self.compaction_pending.set(pending);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::CURRENT_SUPER_VERSION_NUMBER)
        {
            if let Ok(num) = value.parse::<i64>() {
                self.current_super_version_number.set(num);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::MIN_LOG_NUMBER_TO_KEEP)
        {
            if let Ok(num) = value.parse::<i64>() {
                self.min_log_number_to_keep.set(num);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::MIN_OBSOLETE_SST_NUMBER_TO_KEEP)
        {
            if let Ok(num) = value.parse::<i64>() {
                self.min_obsolete_sst_number_to_keep.set(num);
            }
        }

        if let Ok(Some(value)) = self.db.property_value(rocksdb::properties::BASE_LEVEL) {
            if let Ok(level) = value.parse::<i64>() {
                self.base_level.set(level);
            }
        }

        if let Ok(Some(value)) = self
            .db
            .property_value(rocksdb::properties::BLOCK_CACHE_CAPACITY)
        {
            if let Ok(capacity) = value.parse::<i64>() {
                self.block_cache_capacity.set(capacity);
            }
        }

        {
            let builder = &mut rocksdb::perf::MemoryUsageBuilder::new().unwrap();
            builder.add_tx_db(&self.db);
            options::add_cache(builder);

            let approximate_mem_table_total =
                builder.build().unwrap().approximate_mem_table_total();
            self.approximate_mem_table_total
                .set(approximate_mem_table_total.try_into().unwrap());

            let approximate_mem_table_unflushed =
                builder.build().unwrap().approximate_mem_table_unflushed();
            self.approximate_mem_table_unflushed
                .set(approximate_mem_table_unflushed.try_into().unwrap());

            let approximate_mem_table_readers_total = builder
                .build()
                .unwrap()
                .approximate_mem_table_readers_total();
            self.approximate_mem_table_readers_total
                .set(approximate_mem_table_readers_total.try_into().unwrap());

            let approximate_cache_total = builder.build().unwrap().approximate_cache_total();
            self.approximate_cache_total
                .set(approximate_cache_total.try_into().unwrap());
        }
    }

    fn gather_metrics(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = prometheus::gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}

pub async fn serve(chain_db: ChainDB) -> std::io::Result<()> {
    const ADDRESS: &str = "[::]:9090";

    info!("Starting prometheus metrics server: {}", ADDRESS);
    let metrics_data = web::Data::new(Metrics::new(&chain_db.clone()));

    tokio::task::spawn(async move {
        HttpServer::new(move || {
            App::new()
                .app_data(metrics_data.clone())
                .route("/", web::get().to(index))
                .route("/metrics", web::get().to(metrics))
                .route("/stats", web::get().to(stats))
                .route("/sstables", web::get().to(sstables))
                .route("/cfstats", web::get().to(cfstats))
                .route("/dbstats", web::get().to(dbstats))
                .route("/levelstats", web::get().to(levelstats))
                .route("/options_statistics", web::get().to(options_statistics))
        })
        .bind(ADDRESS)
        .expect("Failed to bind address")
        .run()
        .await
        .expect("Server run failed");
    });

    Ok(())
}

async fn index(_metrics: web::Data<Metrics>) -> impl Responder {
    HttpResponse::Ok().body("Prometheus metrics server.")
}

async fn metrics(metrics: web::Data<Metrics>) -> impl Responder {
    metrics.update_metrics();
    let metrics = metrics.gather_metrics();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(metrics)
}

async fn stats(metrics: web::Data<Metrics>) -> impl Responder {
    let stats = metrics
        .db
        .property_value(rocksdb::properties::STATS)
        .unwrap()
        .unwrap();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(stats)
}

async fn sstables(metrics: web::Data<Metrics>) -> impl Responder {
    let stats = metrics
        .db
        .property_value(rocksdb::properties::SSTABLES)
        .unwrap()
        .unwrap();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(stats)
}

async fn cfstats(metrics: web::Data<Metrics>) -> impl Responder {
    let stats = metrics
        .db
        .property_value(rocksdb::properties::CFSTATS)
        .unwrap()
        .unwrap();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(stats)
}

async fn dbstats(metrics: web::Data<Metrics>) -> impl Responder {
    let stats = metrics
        .db
        .property_value(rocksdb::properties::DBSTATS)
        .unwrap()
        .unwrap();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(stats)
}

async fn levelstats(metrics: web::Data<Metrics>) -> impl Responder {
    let stats = metrics
        .db
        .property_value(rocksdb::properties::LEVELSTATS)
        .unwrap()
        .unwrap();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(stats)
}

async fn options_statistics(metrics: web::Data<Metrics>) -> impl Responder {
    let stats = metrics
        .db
        .property_value(rocksdb::properties::OPTIONS_STATISTICS)
        .unwrap()
        .unwrap();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(stats)
}
