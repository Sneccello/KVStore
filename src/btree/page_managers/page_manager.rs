use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use crate::btree::btree_node::BTreeNode;
use crate::btree::common::PageId;
use crate::errors::{KvError, KvResult};

pub trait PageManager: Send + Sync{
    fn get_node(&self, page: PageId) -> KvResult<Arc<RwLock<BTreeNode>>>;
    fn alloc_node(&self, node: BTreeNode) ->KvResult<PageId>;

    fn get_pages(&self) -> HashMap<PageId, Arc<RwLock<BTreeNode>>>;

    fn delete(&self, page: PageId) -> KvResult<()>;

    fn sync(&self) -> KvResult<()>;
    fn mark_dirty(&self, page_id: PageId) -> KvResult<()>;
}

