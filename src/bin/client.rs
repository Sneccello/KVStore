use std::sync::{Arc};
use std::{fmt};
use std::collections::{HashMap};
use std::time::{Duration, Instant};
use rand::distributions::Alphanumeric;
use rand::Rng;
use rand::rngs::ThreadRng;
use tokio::time::interval;
use reqwest::{Client, StatusCode};
use serde::Serialize;
use kv_store::logging::AsyncLogger;


const MAX_KEY_LEN: usize = 128;

#[derive(Serialize, PartialEq)]
enum Method{
    Get,
    Put,
    Delete
}
impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Method::Get => write!(f, "GET"),
            Method::Put => write!(f, "PUT"),
            Method::Delete => write!(f, "DELETE"),
        }
    }
}


#[derive(Serialize)]
struct RequestResult{
    request_type: String,
    duration: Duration,
    status: u16,
    error_msg: Option<String>,
    url: String,
}

#[derive(Serialize)]
struct Measurement{
    request_result: RequestResult,
    current_store_size: usize,
    current_qps: i32
}

fn generate_string(rng: &mut ThreadRng) -> String {
    let len = rng.gen_range(1..MAX_KEY_LEN);
    rng.sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}

fn generate_key_values(dataset_size: usize) -> (Vec<String>, Vec<String>){
    let mut keys: Vec<String> = Vec::with_capacity(dataset_size);
    let mut values: Vec<String> = Vec::with_capacity(dataset_size);

    let rng = &mut rand::thread_rng();

    for _ in 0..dataset_size{

        let k: String = generate_string(rng);
        let v: String = generate_string(rng);

        keys.push(k);
        values.push(v);
    }
    (keys, values)

}


async fn timed_request(url: String, value: String, method: Method, client: &Client) -> RequestResult{

    let time = Instant::now();
    let mut error_msg = None;
    let status: Option<StatusCode>;
    let res = match method {
        Method::Get => {client.get(&url).send().await},
        Method::Put => {client.put(&url).body(value.clone()).send().await},
        Method::Delete => {client.delete(&url).send().await},
    };

    let duration = time.elapsed();
    match res {
        Ok(result) => {
            status = Some(result.status());
            let bytes = result.bytes().await.unwrap();
            if status != Some(StatusCode::OK) {
                error_msg = Some(String::from_utf8_lossy(&bytes).to_string());
            } else if method == Method::Get{
                let resp = String::from_utf8_lossy(&bytes);

                if resp  != "" && resp != value {
                    let msg = format!(
                        "Value error. Expected {} but got {}", value, resp
                    );
                    error_msg = Some(msg);
                }

            }
        },
        Err(err) => {
            status = err.status();
            error_msg = Some(err.to_string());
        }
    };

    RequestResult{
        request_type: method.to_string(),
        duration,
        status: status.map(|c| c.as_u16()).or_else(|| Some(0)).unwrap(),
        error_msg,
        url,
    }

}

async fn load_store(base_url: &str, client: &Client, size: usize) -> (Vec<String>, Vec<String>){
    let (keys, values) = generate_key_values(size);

    for (key, value) in keys.iter().zip(values.iter()){
        let url =  format!("{}/{}", base_url, key);
        timed_request(
            url, value.clone(), Method::Put, client
        ).await;
    }

    (keys, values)
}

#[tokio::main]
async fn main() {

    let data_logger = AsyncLogger::<Measurement>::new("client_data.log".into(), 10_000);
    let msg_logger = AsyncLogger::<Measurement>::new("client_msg_data.log".into(), 10_000);

    let data_logger_arc = Arc::new(data_logger);

    let client = Client::new();
    let initial_size = 1000;
    let qps_increment = 100;
    let max_qps = 20_000;
    let qps_tier_duration = 10;
    let read_probability = 0.6;
    let write_is_put_probability = 0.75;

    let base_url = "http://127.0.0.1:3000/kv";

    msg_logger.log_msg("Loading store..").await;
    let (mut keys, mut values) = load_store(base_url, &client, initial_size).await;

    let mut key2index: HashMap<String, usize> = HashMap::from_iter(
        keys.iter().zip(0..keys.len()).map(|(k, v)| (k.clone(), v))
    );

    msg_logger.log_msg("Loading store DONE").await;


    let mut current_qps = 1;
    let mut rng = rand::thread_rng();

    loop {
        let interval_duration = Duration::from_secs_f64(1.0 / current_qps as f64);

        let mut ticker = interval(interval_duration);

        let tier_end = Instant::now() + Duration::from_secs(qps_tier_duration);

        while Instant::now() < tier_end {
            tokio::select! {
                _ = ticker.tick() => {


                    let client_clone = client.clone();
                    let logger_clone = Arc::clone(&data_logger_arc);
                    let is_read = rng.gen_bool(read_probability);
                    let put_write = rng.gen_bool(write_is_put_probability);

                    let idx = rng.gen_range(0..keys.len());


                    let (method, key, value) = match (is_read, put_write) {
                        (true, _) => {
                            let key = keys[idx].clone();
                            let value = values[idx].clone();
                            (Method::Get, key, value)
                        },
                        (false, true) => {
                            let key = generate_string(&mut rng);
                            let value = generate_string(&mut rng);

                            match key2index.get(&key){
                                Some(index) => {
                                    values[*index] = value.clone();
                                }
                                None => {
                                    values.push(value.clone());
                                    keys.push(key.clone());
                                    key2index.insert(key.clone(), keys.len() - 1);
                                }
                            };
                            (Method::Put, key, value)
                        },
                        (false, false) => {
                            let existing_key = keys[idx].clone();
                            keys.remove(idx);
                            let value = values.remove(idx);
                            (Method::Delete, existing_key, value)
                        }
                    };

                    let url = format!("{}/{}", base_url, key);

                    let current_size = keys.len();

                    tokio::spawn(async move {

                        let record = timed_request(
                            url.clone(), value, method, &client_clone
                        ).await;
                        let measurement = Measurement{
                            request_result: record,
                            current_qps,
                            current_store_size: current_size,
                        };
                        logger_clone.log(measurement).await
                    });
                }
            }
        }
        current_qps += qps_increment;

        if current_qps > max_qps{
            break;
        }
        msg_logger.log_msg(format!("Switched to {current_qps} QPS.").as_str()).await;
    }
}