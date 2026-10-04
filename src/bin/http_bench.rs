use std::sync::Arc;
use std::time::{Duration, Instant};
use futures::stream::{self, StreamExt};
use rand::prelude::StdRng;
use rand::{SeedableRng};
use tokio::time::interval;
use reqwest::Client;
use kv_store::benchmark_utils::{generate_key_values, next_operation, LatencyCollector, Method, MethodSummary, TestConfig, LOG_FOLDER};
use kv_store::logging::{ItemLogger, Logger};

async fn timed_request(url: String, value: String, method: Method, client: &Client) -> u64 {
    let start = Instant::now();

    let _res = match method {
        Method::Get => client.get(&url).send().await,
        Method::Put => client.put(&url).body(value).send().await,
        Method::Delete => client.delete(&url).send().await,
    };

    start.elapsed().as_nanos() as u64
}

async fn load_store(base_url: &str, client: &Client, size: usize) -> (Vec<String>, Vec<String>) {
    let (keys, values) = generate_key_values(size);

    let max_concurrent_requests = 100;

    let requests = keys.iter().zip(values.iter()).map(|(key, value)| {
        let url = format!("{}/{}", base_url, key);
        timed_request(url, value.clone(), Method::Put, client)
    });

    stream::iter(requests)
        .buffer_unordered(max_concurrent_requests)
        .collect::<Vec<_>>()
        .await;

    (keys, values)
}

#[tokio::main]
async fn main() {
    let config = TestConfig::new("configs/wal_write.yaml");


    let get_log_path = std::path::Path::new(LOG_FOLDER).join("client_get_summary.csv");
    let get_logger = Arc::new(ItemLogger::<MethodSummary>::new(get_log_path.to_str().unwrap(), 10_000).await);

    let put_log_path = std::path::Path::new(LOG_FOLDER).join("client_put_summary.csv");
    let put_logger = Arc::new(ItemLogger::<MethodSummary>::new(put_log_path.to_str().unwrap(), 10_000).await);

    let del_log_path = std::path::Path::new(LOG_FOLDER).join("client_delete_summary.csv");
    let del_logger = Arc::new(ItemLogger::<MethodSummary>::new(del_log_path.to_str().unwrap(), 10_000).await);

    let collector = Arc::new(LatencyCollector::new());
    let client = Client::new();
    let base_url = "http://127.0.0.1:3000/kv";

    println!("Loading store with {} keys...", config.server_config.initial_size);
    let (mut keys, mut values) = load_store(base_url, &client, config.server_config.initial_size).await;
    println!("Loading store DONE");

    let mut current_qps: u64 = 1;
    let mut rng = StdRng::from_entropy();

    loop {
        let interval_duration = Duration::from_secs_f64(1.0 / current_qps as f64);
        let mut ticker = interval(interval_duration);
        let tier_end = Instant::now() + Duration::from_secs(config.client_config.qps_tier_duration);

        while Instant::now() < tier_end {
            tokio::select! {
                _ = ticker.tick() => {
                    let client_clone = client.clone();
                    let collector_clone = collector.clone();

                    let (method, key, value) = next_operation(&mut rng, &mut keys, &mut values, config.client_config.load_type);

                    let url = format!("{}/{}", base_url, key);

                    tokio::spawn(async move {
                        let duration_ns = timed_request(url, value, method, &client_clone).await;
                        collector_clone.record(&method, duration_ns, duration_ns);
                    });
                }
            }
        }

        let (get_sum, put_sum, del_sum) = collector.flush_and_reset(current_qps);
        if let Some(s) = get_sum { get_logger.log_item(s).unwrap(); }
        if let Some(s) = put_sum { put_logger.log_item(s).unwrap(); }
        if let Some(s) = del_sum { del_logger.log_item(s).unwrap(); }

        current_qps += config.client_config.qps_increment;

        if current_qps > config.client_config.max_qps {
            break;
        }
        println!("Switched to {current_qps} QPS.");
    }
}