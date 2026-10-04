use std::sync::Arc;
use std::time::{Duration, Instant};
use futures::{stream, StreamExt};
use rand::rngs::StdRng;
use rand::{SeedableRng};
use tokio::time::interval;
use kv_store::benchmark_utils::{generate_key_values, next_operation, DurabilityMode, LatencyCollector, LoadType, Method, MethodSummary, TestConfig};
use kv_store::btree::BTree;
use kv_store::btree::btree::BTreeLogItem;
use kv_store::btree::page_managers::persistent_page_manager::{syncing_loop, PageManagerLogItem, PersistentPageManager};
use kv_store::engine::StorageEngine;
use kv_store::logging::{ItemLogger, Logger};


async fn run_worker_tier(
    worker_data: Vec<(Vec<String>, Vec<String>)>,
    target_qps: f64,
    duration: Duration,
    load_type: LoadType,
    tree_arc: Arc<BTree>,
    collector: Option<Arc<LatencyCollector>>,
) -> Result<Vec<(Vec<String>, Vec<String>)>, Box<dyn std::error::Error>> {
    let num_workers = worker_data.len();
    let worker_qps = target_qps / (num_workers as f64);
    let interval_duration = Duration::from_secs_f64(1.0 / worker_qps.max(0.0001));
    let tier_end = Instant::now() + duration;

    let mut handles = Vec::with_capacity(num_workers);

    for (mut w_keys, mut w_values) in worker_data.into_iter() {
        let collector_clone = collector.clone();
        let tree_clone = tree_arc.clone();

        let handle = tokio::spawn(async move {
            let mut rng = StdRng::from_entropy();
            let mut ticker = interval(interval_duration);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);

            while Instant::now() < tier_end {
                let scheduled_tick = ticker.tick().await;

                let (method, key, value) = next_operation(&mut rng, &mut w_keys, &mut w_values, load_type);

                let start_exec = Instant::now();
                match method {
                    Method::Get => {
                        let _ = tree_clone.get(key.as_bytes());
                    }
                    Method::Put => {
                        let _ = tree_clone.set(key.as_bytes(), value.as_bytes()).await;
                    }
                    Method::Delete => {
                        let _ = tree_clone.delete(key.as_bytes()).await;
                    }
                }
                let exec_ns = start_exec.elapsed().as_nanos() as u64;
                let e2e_ns = scheduled_tick.elapsed().as_nanos() as u64;

                if let Some(ref coll) = collector_clone {
                    coll.record(&method, exec_ns, e2e_ns);
                }
            }

            (w_keys, w_values)
        });

        handles.push(handle);
    }

    let mut next_data = Vec::with_capacity(num_workers);
    for handle in handles {
        next_data.push(handle.await?);
    }
    Ok(next_data)
}

async fn load_store(tree: &BTree, size: usize) -> (Vec<String>, Vec<String>) {
    let (keys, values) = generate_key_values(size);

    let max_concurrent_requests = 100;

    let requests = keys.iter().zip(values.iter()).map(|(key, value)| async move {
        tree.set_operation(key.as_bytes(), value.as_bytes()).await
    });

    stream::iter(requests)
        .buffer_unordered(max_concurrent_requests)
        .collect::<Vec<_>>()
        .await;

    (keys, values)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = std::env::args().nth(1).unwrap_or_else(|| "configs/wal_write.yaml".to_string());
    let config = TestConfig::new(&config_path);
    const NUM_WORKERS: usize = 1024;

    let log_folder = &config.log_folder;
    std::fs::create_dir_all(log_folder).ok();

    let pm_log_data_path = std::path::Path::new(log_folder).join("page_manager_data.csv");
    let pm_data_path_s = pm_log_data_path.to_str().unwrap();
    let pm_data_logger = Arc::new(ItemLogger::<PageManagerLogItem>::new(pm_data_path_s, 10_000).await);

    // 3 dedicated summary loggers for Get, Put, and Delete
    let get_log_path = std::path::Path::new(log_folder).join("mem_get_summary.csv");
    let get_logger = Arc::new(ItemLogger::<MethodSummary>::new(get_log_path.to_str().unwrap(), 10_000).await);

    let put_log_path = std::path::Path::new(log_folder).join("mem_put_summary.csv");
    let put_logger = Arc::new(ItemLogger::<MethodSummary>::new(put_log_path.to_str().unwrap(), 10_000).await);

    let del_log_path = std::path::Path::new(log_folder).join("mem_delete_summary.csv");
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

    let tree_log_data_path = std::path::Path::new(log_folder).join("tree_operations.csv");
    let tree_data_path_s = tree_log_data_path.to_str().unwrap();
    let tree_data_logger = Arc::new(ItemLogger::<BTreeLogItem>::new(tree_data_path_s, 50_000).await);
    let tree = BTree::new(page_manager, config.server_config.page_size, tree_data_logger);
    let tree_arc = Arc::new(tree);

    println!("Loading store with {} keys...", config.server_config.initial_size);
    let (keys, values) = load_store(&tree_arc, config.server_config.initial_size).await;
    println!("Loading store DONE");

    let chunk_size = (keys.len() + NUM_WORKERS - 1) / NUM_WORKERS;
    let mut worker_data: Vec<(Vec<String>, Vec<String>)> = Vec::with_capacity(NUM_WORKERS);
    let mut keys_iter = keys.into_iter();
    let mut values_iter = values.into_iter();

    for _ in 0..NUM_WORKERS {
        let k_chunk: Vec<String> = keys_iter.by_ref().take(chunk_size).collect();
        let v_chunk: Vec<String> = values_iter.by_ref().take(chunk_size).collect();
        worker_data.push((k_chunk, v_chunk));
    }

    let mut current_qps = config.client_config.qps_increment;

    loop {
        worker_data = run_worker_tier(
            worker_data,
            current_qps as f64,
            Duration::from_secs(config.client_config.qps_tier_duration),
            config.client_config.load_type,
            tree_arc.clone(),
            Some(collector.clone()),
        ).await?;

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