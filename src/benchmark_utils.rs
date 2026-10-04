use std::fmt;
use std::fs::File;
use rand::distributions::Alphanumeric;
use rand::prelude::{StdRng};
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum LoadType{
    ReadDominant,
    WriteDominant,
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

    pub mean_exec_ns: f64,
    pub p50_exec_ns: u64,
    pub p90_exec_ns: u64,
    pub p99_exec_ns: u64,
    pub p999_exec_ns: u64,
    pub max_exec_ns: u64,

    pub mean_e2e_ns: f64,
    pub p50_e2e_ns: u64,
    pub p90_e2e_ns: u64,
    pub p99_e2e_ns: u64,
    pub p999_e2e_ns: u64,
    pub max_e2e_ns: u64,
}

use hdrhistogram::Histogram;
use std::sync::Mutex;

pub struct LatencyCollector {
    get_exec_hist: Mutex<Histogram<u64>>,
    get_e2e_hist: Mutex<Histogram<u64>>,
    put_exec_hist: Mutex<Histogram<u64>>,
    put_e2e_hist: Mutex<Histogram<u64>>,
    del_exec_hist: Mutex<Histogram<u64>>,
    del_e2e_hist: Mutex<Histogram<u64>>,
}

impl LatencyCollector {
    pub fn new() -> Self {
        Self {
            get_exec_hist: Mutex::new(Histogram::<u64>::new(3).unwrap()),
            get_e2e_hist: Mutex::new(Histogram::<u64>::new(3).unwrap()),
            put_exec_hist: Mutex::new(Histogram::<u64>::new(3).unwrap()),
            put_e2e_hist: Mutex::new(Histogram::<u64>::new(3).unwrap()),
            del_exec_hist: Mutex::new(Histogram::<u64>::new(3).unwrap()),
            del_e2e_hist: Mutex::new(Histogram::<u64>::new(3).unwrap()),
        }
    }

    pub fn record(&self, method: &Method, exec_ns: u64, e2e_ns: u64) {
        match method {
            Method::Get => {
                let _ = self.get_exec_hist.lock().unwrap().record(exec_ns);
                let _ = self.get_e2e_hist.lock().unwrap().record(e2e_ns);
            }
            Method::Put => {
                let _ = self.put_exec_hist.lock().unwrap().record(exec_ns);
                let _ = self.put_e2e_hist.lock().unwrap().record(e2e_ns);
            }
            Method::Delete => {
                let _ = self.del_exec_hist.lock().unwrap().record(exec_ns);
                let _ = self.del_e2e_hist.lock().unwrap().record(e2e_ns);
            }
        };
    }

    pub fn flush_and_reset(&self, current_qps: u64) -> (Option<MethodSummary>, Option<MethodSummary>, Option<MethodSummary>) {
        let summarize = |exec_h: &mut Histogram<u64>, e2e_h: &mut Histogram<u64>| -> Option<MethodSummary> {
            if exec_h.len() == 0 {
                return None;
            }
            let summary = MethodSummary {
                timestamp_nanos: get_unix_nano(),
                current_qps,
                total_ops: exec_h.len(),
                mean_exec_ns: exec_h.mean(),
                p50_exec_ns: exec_h.value_at_quantile(0.50),
                p90_exec_ns: exec_h.value_at_quantile(0.90),
                p99_exec_ns: exec_h.value_at_quantile(0.99),
                p999_exec_ns: exec_h.value_at_quantile(0.999),
                max_exec_ns: exec_h.max(),
                mean_e2e_ns: e2e_h.mean(),
                p50_e2e_ns: e2e_h.value_at_quantile(0.50),
                p90_e2e_ns: e2e_h.value_at_quantile(0.90),
                p99_e2e_ns: e2e_h.value_at_quantile(0.99),
                p999_e2e_ns: e2e_h.value_at_quantile(0.999),
                max_e2e_ns: e2e_h.max(),
            };
            exec_h.reset();
            e2e_h.reset();
            Some(summary)
        };

        let mut get_exec = self.get_exec_hist.lock().unwrap();
        let mut get_e2e = self.get_e2e_hist.lock().unwrap();
        let mut put_exec = self.put_exec_hist.lock().unwrap();
        let mut put_e2e = self.put_e2e_hist.lock().unwrap();
        let mut del_exec = self.del_exec_hist.lock().unwrap();
        let mut del_e2e = self.del_e2e_hist.lock().unwrap();

        (
            summarize(&mut get_exec, &mut get_e2e),
            summarize(&mut put_exec, &mut put_e2e),
            summarize(&mut del_exec, &mut del_e2e),
        )
    }
}


pub fn generate_string(rng: &mut (impl Rng + ?Sized)) -> String {
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


#[inline]
pub fn next_operation(
    rng: &mut StdRng,
    w_keys: &mut Vec<String>,
    w_values: &mut Vec<String>,
    load_type: LoadType,
) -> (Method, String, String) {
    if w_keys.is_empty() {
        let key = generate_string(rng);
        let value = generate_string(rng);
        w_keys.push(key);
        w_values.push(value);
    }

    match load_type {
        LoadType::ReadDominant => {
            let idx = rng.gen_range(0..w_keys.len());
            let is_read = rng.gen_bool(0.9);
            let method = if is_read { Method::Get } else { Method::Put };
            let key = w_keys[idx].clone();
            let value = if is_read { w_values[idx].clone() } else { generate_string(rng) };
            (method, key, value)
        }
        LoadType::WriteDominant => {
            let idx = rng.gen_range(0..w_keys.len());
            let is_read = rng.gen_bool(0.1);
            let write_is_delete = rng.gen_bool(0.5);
            if is_read {
                let key = w_keys[idx].clone();
                let value = w_values[idx].clone();
                (Method::Get, key, value)
            } else if write_is_delete && !w_keys.is_empty() {
                let key = w_keys.swap_remove(idx);
                let value = w_values.swap_remove(idx);
                (Method::Delete, key, value)
            } else {
                let key = generate_string(rng);
                let new_value = generate_string(rng);
                w_keys.push(key.clone());
                w_values.push(new_value.clone());
                (Method::Put, key, new_value)
            }
        }
    }
}