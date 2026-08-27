use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::Arc;
use chrono::Utc;
use serde::Serialize;
use tokio::sync::{mpsc, Mutex};

pub struct AsyncLogger<T> {
    sender: mpsc::Sender<T>,
    writer: Arc<Mutex<BufWriter<File>>>,
}


impl  <T: Serialize+Send+'static> AsyncLogger<T> {
    pub fn new(file_path: String, buffer_capacity: usize) -> Self{
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


    pub async fn log(&self, item: T){
        if let Err(e) = self.sender.send(item).await {
            eprintln!("Error sending log message: {}", e);
        }
    }

    pub async fn log_msg(&self, msg: &str){
        self.writer.lock().await.write(
            format!("{}:{}\n",
                Utc::now().to_rfc3339(),
                msg).as_bytes()
        ).unwrap();

        self.writer.lock().await.flush().unwrap();
        println!("{}", msg);

    }
}
