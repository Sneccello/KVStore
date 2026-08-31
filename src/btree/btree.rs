use std::sync::{Arc, RwLock};
use serde::Serialize;
use crate::btree::btree_node::{BTreeNode};
use crate::btree::common::{get_unix_nano, PageId};
use crate::btree::leaf_node::LeafNode;
use crate::btree::page_managers::page_manager::PageManager;
use crate::logging::{ItemLogger, Logger};


#[derive(Serialize)]
pub enum OperationType{
    Get,
    Delete,
    Set,
}

#[derive(Serialize)]
pub struct BTreeLogItem{
    operation_type: OperationType,
    start_timestamp_nanos: u128,
    duration: u128,
}

pub struct BTree{
    pub root: RwLock<PageId>,
    pub node_fat_limit_bytes: u16,
    pub node_thin_limit_bytes: u16, //TODO
    pub page_manager: Arc<dyn PageManager>,
    pub logger: Arc<dyn Logger<BTreeLogItem>>
}

impl BTree{
    pub fn new(page_manager: Arc<dyn PageManager>, useful_page_size: u16, logger: Arc<dyn Logger<BTreeLogItem>>) -> Self {

        let root = LeafNode::new();
        let root_page = page_manager.alloc_node(BTreeNode::Leaf(root)).unwrap();
        BTree{
            root: RwLock::new(root_page),
            node_fat_limit_bytes: useful_page_size,
            node_thin_limit_bytes: useful_page_size / 2,
            page_manager,
            logger
        }
    }

    pub(crate) fn log_operation(&self, operation_type: OperationType, duration: u128){
        
        self.logger.log_item(
            BTreeLogItem{
                operation_type,
                start_timestamp_nanos: get_unix_nano(),
                duration,
            }
        ).unwrap();
    }

}

