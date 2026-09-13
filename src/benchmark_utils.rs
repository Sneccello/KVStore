use std::fmt;
use std::fs::File;
use rand::distributions::Alphanumeric;
use rand::prelude::ThreadRng;
use rand::Rng;
use serde::{Deserialize, Serialize};

pub const LOG_FOLDER: &str = "logs";

pub const MAX_KEY_LEN: usize = 128;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DurabilityMode {
    AlwaysSync,
    NeverSync,
    PeriodicSync,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerConfig{
    pub initial_size: usize,
    pub page_size: usize,
    pub wal_enabled: bool,
    pub durability_mode: DurabilityMode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientConfig{
    pub qps_increment: u64,
    pub max_qps: u64,
    pub qps_tier_duration: u64,
    pub load_type: LoadType,
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestConfig{
    pub client_config: ClientConfig,
    pub server_config: ServerConfig,
    pub log_folder: String,
}


impl TestConfig{
    pub fn new(config_path: &str) -> TestConfig{
        let file = File::open(config_path).unwrap();

        serde_yaml::from_reader(file).unwrap()

    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Method{
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LoadType{
    ReadDominant,
    WriteDominant,
    Balanced,
}

#[derive(Serialize)]
pub struct Measurement{
    pub request_type: String,
    pub duration: u128,
    pub status: u16,
    pub error_msg: Option<String>,
    pub url: String,
    pub current_qps: u64,
}

use crate::btree::common::get_unix_nano;

#[derive(Serialize, Clone, Debug)]
pub struct MethodSummary {
    pub timestamp_nanos: u128,
    pub current_qps: u64,
    pub total_ops: u64,
    pub mean_ns: f64,
    pub p50_ns: u64,
    pub p90_ns: u64,
    pub p99_ns: u64,
    pub p999_ns: u64,
    pub max_ns: u64,
}

use hdrhistogram::Histogram;
use std::sync::Mutex;

pub struct LatencyCollector {
    get_hist: Mutex<Histogram<u64>>,
    put_hist: Mutex<Histogram<u64>>,
    delete_hist: Mutex<Histogram<u64>>,
}

impl LatencyCollector {
    pub fn new() -> Self {
        Self {
            get_hist: Mutex::new(Histogram::<u64>::new(3).unwrap()),
            put_hist: Mutex::new(Histogram::<u64>::new(3).unwrap()),
            delete_hist: Mutex::new(Histogram::<u64>::new(3).unwrap()),
        }
    }

    pub fn record(&self, method: &Method, duration_ns: u64) {
        let mut hist = match method {
            Method::Get => self.get_hist.lock().unwrap(),
            Method::Put => self.put_hist.lock().unwrap(),
            Method::Delete => self.delete_hist.lock().unwrap(),
        };
        let _ = hist.record(duration_ns);
    }

    pub fn flush_and_reset(&self, current_qps: u64) -> (Option<MethodSummary>, Option<MethodSummary>, Option<MethodSummary>) {
        let summarize = |hist: &mut Histogram<u64>| -> Option<MethodSummary> {
            if hist.len() == 0 {
                return None;
            }
            let summary = MethodSummary {
                timestamp_nanos: get_unix_nano(),
                current_qps,
                total_ops: hist.len(),
                mean_ns: hist.mean(),
                p50_ns: hist.value_at_quantile(0.50),
                p90_ns: hist.value_at_quantile(0.90),
                p99_ns: hist.value_at_quantile(0.99),
                p999_ns: hist.value_at_quantile(0.999),
                max_ns: hist.max(),
            };
            hist.reset();
            Some(summary)
        };

        let mut get_h = self.get_hist.lock().unwrap();
        let mut put_h = self.put_hist.lock().unwrap();
        let mut del_h = self.delete_hist.lock().unwrap();

        (summarize(&mut get_h), summarize(&mut put_h), summarize(&mut del_h))
    }
}


pub fn generate_string(rng: &mut ThreadRng) -> String {
    let len = rng.gen_range(1..MAX_KEY_LEN);
    rng.sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}

pub fn generate_key_values(dataset_size: usize) -> (Vec<String>, Vec<String>){
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
