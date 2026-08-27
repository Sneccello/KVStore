use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::hash::Hash;
use std::sync::{Arc, LockResult, Mutex, RwLock, RwLockReadGuard};
use std::time::Duration;
use tokio::select;
use tokio::time::interval;
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

pub struct PersistentPageManager{
    allocator: RwLock<PageAllocatorData>,
    flush_data: RwLock<FlushData>,
    block_size: u16,
}

impl PersistentPageManager{

    pub fn new(file_path: &str, block_size: u16) -> PersistentPageManager {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(file_path)
            .unwrap();

        PersistentPageManager::new_with_file(file, block_size)
    }

    fn new_with_file(file: File, block_size: u16) -> PersistentPageManager {
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
        }
    }

    pub fn new_with_temp_file(block_size: u16) -> PersistentPageManager {
        let file = tempfile::tempfile().unwrap();
        PersistentPageManager::new_with_file(file, block_size)
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

        let current_pages = {
            let allocator = self.allocator.write().map_err(|e| LockError())?;
            allocator.pages.clone()
        };


        let mut flush_data = self.flush_data.write().map_err(|e| LockError())?;

        let dirty_pages : Vec<PageId> = flush_data.dirty_pages.drain().collect();
        for page in dirty_pages{
            if !current_pages.contains_key(&page){
                continue;
            }
            let offset = self.get_block_offset(page);

            let node = current_pages.get(&page);
            match node {
                Some(node_ptr) => {
                    let node = node_ptr.read().map_err(|e| LockError())?;
                    write_node(&mut flush_data.file, offset, &node)?;
                },
                None => {
                    return Err(KvError::PageNotFound(page));
                }
            }
        }
        flush_data.file.sync_data().map_err(|e| KvError::IoError(e.to_string()))

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

pub async fn syncing_loop(manager: Arc<Mutex<PersistentPageManager>>, frequency_s: u64){

    let mut ticker = interval(Duration::from_secs(frequency_s));

    select!{
        _ = ticker.tick()=>{
            manager.lock().unwrap().sync().unwrap();
        }
    }
}