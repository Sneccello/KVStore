use std::sync::{Arc, Mutex, RwLock};
use crate::btree::btree_node::{BTreeNode};
use crate::btree::common::{PageId};
use crate::btree::leaf_node::LeafNode;
use crate::errors::{KvError, KvResult};
use crate::btree::page_managers::page_manager::PageManager;
use crate::btree::page_managers::persistent_page_manager::PersistentPageManager;
use crate::errors::KvError::LockError;

pub struct BTree{
    pub root: RwLock<PageId>,
    pub node_fat_limit_bytes: u16,
    pub node_thin_limit_bytes: u16, //TODO
    pub page_manager: Arc<dyn PageManager>,
}

impl BTree{
    pub fn new(page_manager: Arc<dyn PageManager>, useful_page_size: u16) -> Self {

        let root = LeafNode::new();
        let root_page = page_manager.alloc_node(BTreeNode::Leaf(root)).unwrap();
        BTree{
            root: RwLock::new(root_page),
            node_fat_limit_bytes: useful_page_size,
            node_thin_limit_bytes: useful_page_size / 2,
            page_manager,
        }
    }
    
}

