use tokio::fs::{File, OpenOptions};
use std::marker::PhantomData;
use std::sync::Arc;
use csv::WriterBuilder;
use serde::Serialize;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::{mpsc, Mutex};
use crate::btree::common::get_unix_nano;
use crate::errors::{KvError, KvResult};

pub trait Logger<T>: Send + Sync{
    fn log_item(&self, item: T) -> KvResult<()>;
}

#[derive(Serialize)]
struct LogRecord<T> {
    timestamp: u128,
    #[serde(flatten)]
    item: T,
}

pub struct ItemLogger<T> {
    sender: mpsc::Sender<T>
}

#[derive(Serialize)]
pub struct MessageItem{
    pub msg: String, //TODO level
}

impl <T: Serialize+Send+'static> ItemLogger<T> { //TODO refactor
    pub async fn new(file_path: &str, buffer_capacity: usize) -> Self{
        let (sender, mut receiver) = mpsc::channel::<T>(buffer_capacity);
        let path = file_path.to_string();

        tokio::spawn(async move {

            let file = OpenOptions::new().create(true).truncate(true).write(true).open(path).await.unwrap();


            let mut writer = BufWriter::with_capacity(64 * 1024, file);
            let mut batch = Vec::with_capacity(512);

            let mut write_buf = Vec::with_capacity(64 * 1024);


            let mut has_written_headers = false;

            while receiver.recv_many(&mut batch, 512).await > 0 {
                write_buf.clear();

                {
                    let mut csv_writer = WriterBuilder::new()
                        .has_headers(!has_written_headers)
                        .from_writer(&mut write_buf);

                    for item in batch.drain(..) {
                        let _ = csv_writer.serialize(item);
                    }
                    let _ = csv_writer.flush();
                }

                has_written_headers = true;
                let _ = writer.write_all(&write_buf).await;
            }
            let _ = writer.flush();
        });

        Self{sender}
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
