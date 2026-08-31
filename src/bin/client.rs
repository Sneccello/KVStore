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
use kv_store::logging::{ItemLogger, MessageItem};
use kv_store::logging::Logger;


const MAX_KEY_LEN: usize = 128;
pub const LOG_FOLDER: &str = "logs";
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

enum LoadType{
    ReadDominant,
    WriteDominant,
    Balanced,
}

#[derive(Serialize)]
struct Measurement{
    request_type: String,
    duration: u128,
    status: u16,
    error_msg: Option<String>,
    url: String,
    current_qps: u32,
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


async fn timed_request(url: String, value: String, method: Method, client: &Client) -> Measurement{

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

    Measurement{
        request_type: method.to_string(),
        duration: duration.as_nanos(),
        status: status.map(|c| c.as_u16()).or_else(|| Some(0)).unwrap(),
        error_msg,
        url,
        current_qps: 0, //placeholder
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


    let log_data_path = std::path::Path::new(LOG_FOLDER).join("client_data.csv");
    let data_path_s = log_data_path.to_str().unwrap();

    let data_logger = ItemLogger::<Measurement>::new(data_path_s.into(), 1_000_000).await;
    let data_logger_arc = Arc::new(data_logger);

    let client = Client::new();
    let initial_size = 100_000;
    let qps_increment = 1000;
    let max_qps = 50_000;
    let qps_tier_duration = 5;
    let load_type = LoadType::ReadDominant;

    let base_url = "http://127.0.0.1:3000/kv";

    println!("Loading store..");
    let (mut keys, mut values) = load_store(base_url, &client, initial_size).await;

    let key2index: HashMap<String, usize> = HashMap::from_iter(
        keys.iter().zip(0..keys.len()).map(|(k, v)| (k.clone(), v))
    );

    println!("Loading store DONE");

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

                    let (method, key, value) = match load_type{
                        LoadType::ReadDominant => {
                            let idx = rng.gen_range(0..keys.len());
                            let is_read = rng.gen_bool(0.9);
                            let method = if is_read {Method::Get} else {Method::Put};
                            let key = keys[idx].clone();
                            let value = if is_read {values[idx].clone()} else {generate_string(&mut rng)};
                            (method, key, value)
                        },
                        LoadType::WriteDominant => {
                            let idx = rng.gen_range(0..keys.len());
                            let is_read = rng.gen_bool(0.1);
                            let method = if is_read {Method::Get} else {Method::Put};
                            let method = if is_read {Method::Get} else {Method::Put};
                            let key = keys[idx].clone();
                            let value = if is_read {values[idx].clone()} else {generate_string(&mut rng)};
                            (method, key, value)
                        },
                        LoadType::Balanced => {
                            let idx = rng.gen_range(0..keys.len());
                            let is_read = rng.gen_bool(0.1);
                            let write_is_delete = rng.gen_bool(0.5);
                            if is_read{
                                let key = keys[idx].clone();
                                let value = values[idx].clone();
                                (Method::Get, key, value)
                            }
                            else if write_is_delete{
                                let key = keys[idx].clone();
                                let value = values[idx].clone();
                                (Method::Delete, key, value)
                            }
                            else {
                                let key = keys[idx].clone();
                                let new_value  = generate_string(&mut rng);
                                (Method::Put, key, new_value)
                            }
                        }
                    };


                    let url = format!("{}/{}", base_url, key);

                    let current_size = keys.len();

                    tokio::spawn(async move {

                        let mut record = timed_request(
                            url.clone(), value, method, &client_clone
                        ).await;
                        record.current_qps = current_qps;
                        logger_clone.log_item(record).unwrap();
                    });
                }
            }
        }
        current_qps += qps_increment;

        if current_qps > max_qps{
            break;
        }
        println!("Switched to {current_qps} QPS.")
    }
}