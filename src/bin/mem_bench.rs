use std::sync::Arc;
use std::time::{Duration, Instant};
use futures::{stream, StreamExt};
use rand::Rng;
use tokio::time::interval;
use kv_store::benchmark_utils::{generate_key_values, generate_string, DurabilityMode, LatencyCollector, LoadType, Method, MethodSummary, TestConfig};
use kv_store::btree::BTree;
use kv_store::btree::btree::BTreeLogItem;
use kv_store::btree::page_managers::persistent_page_manager::{syncing_loop, PageManagerLogItem, PersistentPageManager};
use kv_store::engine::StorageEngine;
use kv_store::logging::{ItemLogger, Logger};

pub const LOG_FOLDER: &str = "logs";

async fn timed_method(
    key: String,
    value: String,
    method: Method,
    tree: &BTree,
) -> u64 {
    let start = Instant::now();

    match method {
        Method::Get => {
            let _ = tree.get(key.as_bytes());
        }
        Method::Put => {
            let _ = tree.set(key.as_bytes(), value.as_bytes()).await;
        }
        Method::Delete => {
            let _ = tree.delete(key.as_bytes()).await;
        }
    }

    start.elapsed().as_nanos() as u64
}

async fn load_store(tree: &BTree, size: usize) -> (Vec<String>, Vec<String>) {
    let (keys, values) = generate_key_values(size);

    let max_concurrent_requests = 100;

    let requests = keys.iter().zip(values.iter()).map(|(key, value)| async move {
        tree.set(key.as_bytes(), value.as_bytes()).await
    });

    stream::iter(requests)
        .buffer_unordered(max_concurrent_requests)
        .collect::<Vec<_>>()
        .await;

    (keys, values)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let config = TestConfig::new("configs/read_heavy_config.yaml");

    let pm_log_data_path = std::path::Path::new(LOG_FOLDER).join("page_manager_data.csv");
    let pm_data_path_s = pm_log_data_path.to_str().unwrap();
    let pm_data_logger = Arc::new(ItemLogger::<PageManagerLogItem>::new(pm_data_path_s, 10_000).await);

    // 3 dedicated summary loggers for Get, Put, and Delete
    let get_log_path = std::path::Path::new(LOG_FOLDER).join("mem_get_summary.csv");
    let get_logger = Arc::new(ItemLogger::<MethodSummary>::new(get_log_path.to_str().unwrap(), 10_000).await);

    let put_log_path = std::path::Path::new(LOG_FOLDER).join("mem_put_summary.csv");
    let put_logger = Arc::new(ItemLogger::<MethodSummary>::new(put_log_path.to_str().unwrap(), 10_000).await);

    let del_log_path = std::path::Path::new(LOG_FOLDER).join("mem_delete_summary.csv");
    let del_logger = Arc::new(ItemLogger::<MethodSummary>::new(del_log_path.to_str().unwrap(), 10_000).await);

    let collector = Arc::new(LatencyCollector::new());

    let page_manager = Arc::new(
        PersistentPageManager::new("kv.db", config.server_config.page_size, pm_data_logger, config.server_config.wal_enabled)
    );
    let pm_copy = page_manager.clone();
    if let DurabilityMode::PeriodicSync = config.server_config.durability_mode {
        tokio::task::spawn(async move {
            syncing_loop(
                pm_copy,
                Duration::from_secs(10),
            ).await
        });
    }

    let tree_log_data_path = std::path::Path::new(LOG_FOLDER).join("tree_operations.csv");
    let tree_data_path_s = tree_log_data_path.to_str().unwrap();
    let tree_data_logger = Arc::new(ItemLogger::<BTreeLogItem>::new(tree_data_path_s, 50_000).await);
    let tree = BTree::new(page_manager, config.server_config.page_size, tree_data_logger);
    let tree_arc = Arc::new(tree);

    println!("Loading store with {} keys...", config.server_config.initial_size);
    let (keys, values) = load_store(&tree_arc, config.server_config.initial_size).await;
    println!("Loading store DONE");

    let mut current_qps = 1;
    let mut rng = rand::thread_rng();

    loop {
        let interval_duration = Duration::from_secs_f64(1.0 / current_qps as f64);
        let mut ticker = interval(interval_duration);
        let tier_end = Instant::now() + Duration::from_secs(config.client_config.qps_tier_duration);

        while Instant::now() < tier_end {
            tokio::select! {
                _ = ticker.tick() => {
                    let (method, key, value) = match config.client_config.load_type {
                        LoadType::ReadDominant => {
                            let idx = rng.gen_range(0..keys.len());
                            let is_read = rng.gen_bool(0.9);
                            let method = if is_read { Method::Get } else { Method::Put };
                            let key = keys[idx].clone();
                            let value = if is_read { values[idx].clone() } else { generate_string(&mut rng) };
                            (method, key, value)
                        },
                        LoadType::WriteDominant => {
                            let idx = rng.gen_range(0..keys.len());
                            let is_read = rng.gen_bool(0.1);
                            let method = if is_read { Method::Get } else { Method::Put };
                            let key = keys[idx].clone();
                            let value = if is_read { values[idx].clone() } else { generate_string(&mut rng) };
                            (method, key, value)
                        },
                        LoadType::Balanced => {
                            let idx = rng.gen_range(0..keys.len());
                            let is_read = rng.gen_bool(0.1);
                            let write_is_delete = rng.gen_bool(0.5);
                            if is_read {
                                let key = keys[idx].clone();
                                let value = values[idx].clone();
                                (Method::Get, key, value)
                            } else if write_is_delete {
                                let key = keys[idx].clone();
                                let value = values[idx].clone();
                                (Method::Delete, key, value)
                            } else {
                                let key = keys[idx].clone();
                                let new_value = generate_string(&mut rng);
                                (Method::Put, key, new_value)
                            }
                        }
                    };

                    let collector_clone = collector.clone();
                    let tree = tree_arc.clone();
                    tokio::spawn(async move {
                        let duration_ns = timed_method(key, value, method, &tree).await;
                        collector_clone.record(&method, duration_ns);
                    });
                }
            }
        }

        // Flush and log 1 summary row per method at the end of the tier
        let (get_sum, put_sum, del_sum) = collector.flush_and_reset(current_qps);
        if let Some(s) = get_sum { get_logger.log_item(s)?; }
        if let Some(s) = put_sum { put_logger.log_item(s)?; }
        if let Some(s) = del_sum { del_logger.log_item(s)?; }

        current_qps += config.client_config.qps_increment;

        if current_qps > config.client_config.max_qps {
            return Ok(());
        }
        println!("Switched to {current_qps} QPS.");
    }
}