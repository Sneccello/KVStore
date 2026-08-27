use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::sync::{Arc, RwLock};
use std::time;
use std::time::Duration;
use chrono::Utc;
use serde::Serialize;
use tokio::select;
use tokio::time::interval;
use crate::logging::{MessageItem};
use crate::logging::{ItemLogger, Logger};
use crate::btree::btree_node::BTreeNode;
use crate::btree::common::PageId;
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
pub struct PageManagerLogItem {
    syncing_timestamp: String,
    syncing_duration: u128,
}

pub struct PersistentPageManager{
    allocator: RwLock<PageAllocatorData>,
    flush_data: RwLock<FlushData>,
    block_size: u16,
    data_logger: Arc<dyn Logger<PageManagerLogItem>>,
    message_logger: Arc<dyn Logger<MessageItem>>,
}

impl PersistentPageManager{

    pub fn new(file_path: &str, block_size: u16,
               data_logger: Arc<dyn Logger<PageManagerLogItem>>,
               message_logger: Arc<dyn Logger<MessageItem>>,
    ) -> PersistentPageManager {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(file_path)
            .unwrap();

        PersistentPageManager::new_with_file(file, block_size, data_logger, message_logger)
    }

    fn new_with_file(file: File, block_size: u16,
                     data_logger: Arc<dyn Logger<PageManagerLogItem>>,
                     message_logger:Arc<dyn Logger<MessageItem>>,
    ) -> PersistentPageManager {
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
            message_logger,
        }
    }

    pub fn new_with_temp_file(block_size: u16,
                              data_logger: Arc<dyn Logger<PageManagerLogItem>>,
                              message_logger: Arc<dyn Logger<MessageItem>>,
    ) -> PersistentPageManager {
        let file = tempfile::tempfile().unwrap();
        PersistentPageManager::new_with_file(file, block_size, data_logger, message_logger)
    }

    fn get_block_offset(&self, page_id: PageId) -> u64{
        (self.block_size as u64) * (page_id as u64)
    }
}

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

        let mut allocator = self.allocator.write().map_err(|e| LockError())?;

        let id = match allocator.free_list.pop(){
            Some(Reverse(id)) => id,
            None => {
                let id = allocator.next_free_page_id;
                allocator.next_free_page_id+=1;
                id
            }
        };
        allocator.pages.insert(id, Arc::new(RwLock::new(node)));

        let mut flush_data = self.flush_data.write().map_err(|e| LockError())?;
        flush_data.dirty_pages.insert(id);

        Ok(id)
    }

    fn get_pages(&self) -> HashMap<PageId, Arc<RwLock<BTreeNode>>>{
        let allocator = self.allocator.read().map_err(|e| LockError()).unwrap();
        allocator.pages.clone()
    }

    fn delete(&self, page: PageId) -> KvResult<()>{

        let mut allocator = self.allocator.write().map_err(|e| LockError())?;

        allocator.pages.remove(&page);
        allocator.free_list.push(Reverse(page));

        Ok(())
    }

    fn sync(&self) -> KvResult<()>{

        let start = time::Instant::now();
        let now = Utc::now().to_rfc3339();


        let dirty_pages = {
            let mut flush_data = self.flush_data.write().map_err(|e| LockError())?;
            let dirty_pages : Vec<PageId> = flush_data.dirty_pages.drain().collect();
            dirty_pages
        };

        let allocator = self.allocator.read().map_err(|e| LockError())?;
        let mut flush_data = self.flush_data.write().map_err(|e| LockError())?;
        for page in dirty_pages{
            if let Some(node_ptr) = allocator.pages.get(&page) {
                let offset = self.get_block_offset(page);
                let node = node_ptr.read().map_err(|e| LockError())?;
                write_node(&mut flush_data.file, offset, &node)?;
            }
        }
        flush_data.file.sync_data().map_err(|e| KvError::IoError(e.to_string()))?;

        let duration = start.elapsed();
        self.data_logger.log_item(
            PageManagerLogItem{
                syncing_timestamp: now,
                syncing_duration: duration.as_nanos(),
            }
        )


    }

    fn mark_dirty(&self, page_id: PageId) -> KvResult<()> {

        match self.flush_data.write(){
            Ok(mut flush_data) => {
                flush_data.dirty_pages.insert(page_id);
                Ok(())
            },
            Err(err) => {
                Err(KvError::LockError())
            }
        }

    }

}

pub async fn syncing_loop(
    manager: Arc<PersistentPageManager>,
    frequency_s: u64,
    data_logger: Arc<dyn Logger<PageManagerLogItem>>,
    error_logger: Arc<dyn Logger<MessageItem>>,
){

    let mut ticker = interval(Duration::from_secs(frequency_s));

    select!{
        _ = ticker.tick()=>{
            _ = manager.sync();
        }
    }
}