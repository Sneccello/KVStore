use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use async_trait::async_trait;
use serde::Serialize;
use tokio::select;
use tokio::sync::{mpsc, oneshot};
use tokio::sync::oneshot::Sender;
use tokio::time::interval;
use crate::btree::btree::OperationType;
use crate::logging::Logger;
use crate::btree::btree_node::BTreeNode;
use crate::btree::common::{get_unix_nano, PageId};
use crate::btree::page_managers::file_utils::write_node;
use crate::btree::page_managers::page_manager::PageManager;
use crate::errors::{KvError, KvResult};
use crate::errors::KvError::{LockError, PageNotFound, TreeLogicError};

struct PageAllocatorData{
    next_free_page_id: PageId,
    free_list: BinaryHeap<Reverse<PageId>>,
    pages: HashMap<PageId, Arc<RwLock<BTreeNode>>>,
}

struct FlushData{
    dirty_pages: HashSet<PageId>,
    file: File,
}

#[derive(Serialize)]
pub struct LogicalWalRecord{
    pub transaction_id: u128,
    pub operation_type: OperationType,
    pub key: Vec<u8>,
    pub value: Vec<u8>, //disregarded for delete for now
}

type WalMessage = (Vec<u8>, Sender<KvResult<()>>);

#[derive(Serialize)]
pub struct PageManagerLogItem {
    start_timestamp_nanos: u128,
    duration: u128,
}

pub struct PersistentPageManager{
    allocator: RwLock<PageAllocatorData>,
    flush_data: RwLock<FlushData>,
    block_size: usize,
    data_logger: Arc<dyn Logger<PageManagerLogItem>>,
    wal_enabled: bool,
    wal_sender: mpsc::Sender<WalMessage>,
}

impl PersistentPageManager{

    pub fn new(file_path: &str, block_size: usize,
               data_logger: Arc<dyn Logger<PageManagerLogItem>>,
               wal_enabled: bool,
    ) -> PersistentPageManager {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(file_path)
            .unwrap();

        PersistentPageManager::new_with_file(file, block_size, data_logger, wal_enabled)
    }

    fn new_with_file(file: File, block_size: usize,
                     data_logger: Arc<dyn Logger<PageManagerLogItem>>,
                     wal_enabled: bool
    ) -> PersistentPageManager {



        let (sender, receiver) = mpsc::channel::<WalMessage>(10_000);
        if wal_enabled {
            Self::spawn_wal_worker(".wal".into(), receiver);
        }
        Self{
            allocator: RwLock::new(
                PageAllocatorData{
                    next_free_page_id: 0,
                    free_list: BinaryHeap::new(),
                    pages: HashMap::new(),
                }
            ),
            flush_data: RwLock::new(FlushData{
                dirty_pages: HashSet::new(),
                file,
            }),
            block_size,
            data_logger,
            wal_enabled,
            wal_sender: sender,
        }
    }

    fn spawn_wal_worker(path: String, mut receiver: mpsc::Receiver<WalMessage>) {
        std::thread::Builder::new().name("wal-flusher".into()).spawn( move || {

            let file = OpenOptions::new().read(true).write(true).create(true).open(&path).unwrap();

            const CHUNK_SIZE: u64 = 64 * 1024 * 1024;
            let mut allocated_size = file.metadata().map(|m| m.len()).unwrap_or(0);
            if allocated_size < CHUNK_SIZE {
                allocated_size = CHUNK_SIZE;
                let _ = file.set_len(allocated_size);
            }

            let mut writer = BufWriter::with_capacity(1024 * 1024, file);

            const MAX_BATCH_SIZE: usize = 1024;
            let mut batch = Vec::with_capacity(MAX_BATCH_SIZE);
            let mut acks: Vec<Sender<KvResult<()>>> = Vec::with_capacity(MAX_BATCH_SIZE);
            let mut write_buf = Vec::with_capacity(1024 * 256); //256 kb buffer
            let mut current_file_offset = 0;
            let mut flush_count: u64 = 0;
            let mut total_records: u64 = 0;
            let mut total_write_nanos: u128 = 0;
            let mut total_sync_nanos: u128 = 0;
            let mut last_report = std::time::Instant::now();

            while receiver.blocking_recv_many(&mut batch, MAX_BATCH_SIZE) > 0 {
                let batch_size = batch.len();
                write_buf.clear();
                acks.clear();

                for (bytes, ack) in batch.drain(..){
                    let len = bytes.len();
                    write_buf.extend_from_slice(&len.to_le_bytes());
                    write_buf.extend_from_slice(&bytes[..]);
                    acks.push(ack);
                }

                const CHUNK_SIZE: u64 = 64 * 1024 * 1024;

                if current_file_offset + write_buf.len() as u64 > allocated_size {
                    allocated_size += CHUNK_SIZE;
                    let _ = writer.get_ref().set_len(allocated_size);
                }
                current_file_offset += write_buf.len() as u64;

                let write_start = std::time::Instant::now();
                let sync_result = (|| -> KvResult<(u128, u128)>{
                    writer.write_all(&write_buf).map_err(|e| KvError::IoError(e.to_string()))?;
                    writer.flush().map_err(|e| KvError::IoError(e.to_string()))?;
                    let write_done = std::time::Instant::now();
                    writer.get_ref().sync_data().map_err(|e| KvError::IoError(e.to_string()))?;
                    let sync_done = std::time::Instant::now();
                    Ok((write_done.duration_since(write_start).as_nanos(), sync_done.duration_since(write_done).as_nanos()))
                })();

                match sync_result {
                    Ok((w_nanos, s_nanos)) => {
                        flush_count += 1;
                        total_records += batch_size as u64;
                        total_write_nanos += w_nanos;
                        total_sync_nanos += s_nanos;

                        for ack in acks.drain(..) {
                            let _ = ack.send(Ok(()));
                        }
                    }
                    Err(ref err) => {
                        for ack in acks.drain(..) {
                            let _ = ack.send(Err(KvError::IoError(err.to_string())));
                        }
                    }
                }

                if last_report.elapsed() >= std::time::Duration::from_secs(1) {
                    let elapsed_sec = last_report.elapsed().as_secs_f64();
                    let flushes_per_sec = flush_count as f64 / elapsed_sec;
                    let avg_batch = if flush_count > 0 { total_records as f64 / flush_count as f64 } else { 0.0 };
                    let avg_write_ms = if flush_count > 0 { (total_write_nanos as f64 / flush_count as f64) / 1_000_000.0 } else { 0.0 };
                    let avg_sync_ms = if flush_count > 0 { (total_sync_nanos as f64 / flush_count as f64) / 1_000_000.0 } else { 0.0 };
                    let wal_qps = total_records as f64 / elapsed_sec;

                    println!(
                        "[WAL Monitor] {:.0} flushes/s | avg batch: {:.1} ops | write: {:.2}ms | sync: {:.2}ms | throughput: {:.0} QPS",
                        flushes_per_sec, avg_batch, avg_write_ms, avg_sync_ms, wal_qps
                    );

                    flush_count = 0;
                    total_records = 0;
                    total_write_nanos = 0;
                    total_sync_nanos = 0;
                    last_report = std::time::Instant::now();
                }
            }

            let _ = writer.flush().map_err(|e| KvError::IoError(e.to_string()));
            let _ = writer.get_ref().sync_data().map_err(|e| KvError::IoError(e.to_string()));
        }).expect("Failed to spawn wal-flusher thread");
    }

    pub fn new_with_temp_file(block_size: usize,
                              data_logger: Arc<dyn Logger<PageManagerLogItem>>,
                              wal_enabled: bool,
    ) -> PersistentPageManager {
        let file = tempfile::tempfile().unwrap();
        PersistentPageManager::new_with_file(file, block_size, data_logger, wal_enabled)
    }

    fn get_block_offset(&self, page_id: PageId) -> u64{
        (self.block_size as u64) * (page_id as u64)
    }
}

#[async_trait]
impl PageManager for PersistentPageManager{

    fn get_node(&self, page: PageId) -> KvResult<Arc<RwLock<BTreeNode>>>{
        match self.allocator.read(){
            Ok(lookup_guard) => {
                lookup_guard.pages.get(&page).cloned()
                    .ok_or_else(|| PageNotFound(page))

            }
            Err(err) => {
                Err(TreeLogicError(err.to_string()))
            }
        }

    }


    fn alloc_node(&self, node: BTreeNode) -> KvResult<PageId> {

        let mut allocator = self.allocator.write().map_err(|_e| LockError())?;

        let id = match allocator.free_list.pop(){
            Some(Reverse(id)) => id,
            None => {
                let id = allocator.next_free_page_id;
                allocator.next_free_page_id+=1;
                id
            }
        };
        allocator.pages.insert(id, Arc::new(RwLock::new(node)));

        let mut flush_data = self.flush_data.write().map_err(|_e| LockError())?;
        flush_data.dirty_pages.insert(id);

        Ok(id)
    }

    fn get_pages(&self) -> HashMap<PageId, Arc<RwLock<BTreeNode>>>{
        let allocator = self.allocator.read().map_err(|_e| LockError()).unwrap();
        allocator.pages.clone()
    }

    fn delete(&self, page: PageId) -> KvResult<()>{

        let mut allocator = self.allocator.write().map_err(|_e| LockError())?;

        allocator.pages.remove(&page);
        allocator.free_list.push(Reverse(page));

        Ok(())
    }

    fn sync(&self) -> KvResult<()>{

        let start = std::time::Instant::now();

        let dirty_pages = {
            let mut flush_data = self.flush_data.write().map_err(|_e| LockError())?;
            let dirty_pages : Vec<PageId> = flush_data.dirty_pages.drain().collect();
            dirty_pages
        };

        let allocator = self.allocator.read().map_err(|_e| LockError())?;
        let mut flush_data = self.flush_data.write().map_err(|_e| LockError())?;
        for page in dirty_pages{
            if let Some(node_ptr) = allocator.pages.get(&page) {
                let offset = self.get_block_offset(page);
                let node = node_ptr.read().map_err(|_e| LockError())?;
                write_node(&mut flush_data.file, offset, &node)?;
            }
        }
        flush_data.file.sync_data().map_err(|e| KvError::IoError(e.to_string()))?;


        self.data_logger.log_item(
            PageManagerLogItem{
                start_timestamp_nanos: get_unix_nano(),
                duration: start.elapsed().as_nanos(),
            }
        )


    }

    fn mark_dirty(&self, page_id: PageId) -> KvResult<()> {

        match self.flush_data.write(){
            Ok(mut flush_data) => {
                flush_data.dirty_pages.insert(page_id);
                Ok(())
            },
            Err(_err) => {
                Err(KvError::LockError())
            }
        }
    }

    async fn add_wal_record(&self, wal_record: LogicalWalRecord) -> KvResult<()>{
        if !self.wal_enabled{
            return Ok(())
        }
        let bytes = bincode::serialize(&wal_record).map_err(|e| KvError::IoError(e.to_string()))?;

        let (ack_tx, ack_rx) = oneshot::channel();
        self.wal_sender
            .send((bytes, ack_tx))
            .await.map_err(
            |e| KvError::IoError(e.to_string())
        )?;
        ack_rx.await.map_err(|e| KvError::IoError(e.to_string()))??;
        Ok(())
    }

}

pub async fn syncing_loop(
    manager: Arc<PersistentPageManager>,
    duration: Duration,
){

    let mut ticker = interval(duration);
    loop {
        select!{
        _ = ticker.tick()=>{
            manager.sync().unwrap();
        }
    }
    }
}