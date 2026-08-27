use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::marker::PhantomData;
use std::sync::Arc;
use chrono::Utc;
use serde::Serialize;
use tokio::sync::{mpsc, Mutex};
use crate::errors::{KvError, KvResult};

pub trait Logger<T>: Send + Sync{
    fn log_item(&self, item: T) -> KvResult<()>;
}

pub struct ItemLogger<T> {
    sender: mpsc::Sender<T>,
    writer: Arc<Mutex<BufWriter<File>>>,
}

#[derive(Serialize)]
pub struct MessageItem{
    pub msg: String, //TODO level
}

impl <T: Serialize+Send+'static> ItemLogger<T> { //TODO refactor
    pub fn new(file_path: &str, buffer_capacity: usize) -> Self{
        let (sender, mut receiver) = mpsc::channel::<T>(buffer_capacity);

        let file = OpenOptions::new().create(true).truncate(true).write(true).open(&file_path).unwrap();

        let writer = Arc::new(Mutex::new(BufWriter::new(file)));
        let worker_writer = Arc::clone(&writer);
        tokio::spawn(async move {

            while let Some(item) = receiver.recv().await.as_mut() {
                if let Ok(mut value) = serde_json::to_value(&item){
                    if let Some(map) = value.as_object_mut(){
                        map.insert(
                            "timestamp".to_string(),
                            serde_json::json!(Utc::now().to_rfc3339())
                        );
                    }

                    if let Ok(mut json_line) = serde_json::to_string(&value) {
                        json_line.push('\n');
                        let _ = worker_writer.lock().await.write(json_line.as_bytes());
                    }
                }
            }
            let _ = worker_writer.lock().await.flush();
        });

        Self{sender, writer: Arc::clone(&writer)}
    }
}

impl <T: Serialize+Send+Sync+'static> Logger<T> for ItemLogger<T>{
    
    fn log_item(&self, item: T) -> KvResult<()> {
        self.sender.try_send(item).map_err(|e| KvError::LoggingError(e.to_string()))
    }
}


pub struct NoopLogger<T>{
    _marker: PhantomData<T>
}

impl<T> NoopLogger<T>{
    pub fn new(_path: String, _buffer_capacity: usize) -> Self{
        Self{
            _marker: PhantomData,
        }
    }
}
impl <T: Serialize+Send+Sync+'static> Logger<T> for NoopLogger<T>{
    fn log_item(&self, _item: T) -> KvResult<()> {
        Ok(())
    }
}
