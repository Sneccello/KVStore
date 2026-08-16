use std::collections::HashMap;

use crate::btree::btree_node::{BTreeNode};
use crate::btree::common::PageId;
use crate::errors::{KvError, KvResult};

pub trait PageManager {
    fn get_node(&self, page: PageId) -> KvResult<&BTreeNode>;
    fn get_node_mut(&mut self, page: PageId) -> KvResult<&mut BTreeNode>;

    fn get_three_mut(&mut self, a: PageId, b: PageId, c: PageId) -> KvResult<(&mut BTreeNode, &mut BTreeNode, &mut BTreeNode)>;

    fn alloc_node(&mut self, node: BTreeNode) -> PageId;

    fn get_pages(&self) -> &HashMap<PageId, BTreeNode>;

    fn delete(&mut self, page: PageId) -> KvResult<()>;
}

pub struct PersistentPageManager{
    page_id: PageId,
    pages: HashMap<PageId, BTreeNode>
}

impl PersistentPageManager {
    pub fn new() -> PersistentPageManager {
        Self{
            page_id: 0,
            pages: HashMap::new()
        }
    }
}

impl PageManager for PersistentPageManager{

    fn get_node(&self, page: PageId) -> KvResult<&BTreeNode>{
        self.pages.get(&page).ok_or_else(|| KvError::PageNotFound(page.clone()))
    }

    fn get_node_mut(&mut self, page: PageId) -> KvResult<&mut BTreeNode>{
        self.pages.get_mut(&page).ok_or_else(|| KvError::PageNotFound(page.clone()))
    }


    fn get_three_mut(&mut self, a: PageId, b: PageId, c: PageId) -> KvResult<(&mut BTreeNode, &mut BTreeNode, &mut BTreeNode)>{
        if a==b || b==c || c==a{
            return Err(KvError::TreeLogicError("Page ids are the same for different nodes".to_string())); //TODO
        }

        let node_a = self.pages.get_mut(&a).ok_or_else(|| KvError::PageNotFound(a.clone()))? as *mut BTreeNode;
        let node_b = self.pages.get_mut(&b).ok_or_else(|| KvError::PageNotFound(b.clone()))? as *mut BTreeNode;
        let node_c = self.pages.get_mut(&c).ok_or_else(|| KvError::PageNotFound(c.clone()))?;

        unsafe{
            Ok((&mut *node_a, &mut *node_b, node_c))
        }
    }

    fn alloc_node(&mut self, node: BTreeNode) -> PageId {
        let id = self.page_id;
        self.page_id += 1;
        self.pages.insert(id, node);
        id
    }

    fn get_pages(&self) -> &HashMap<PageId, BTreeNode>{
        &self.pages
    }

    fn delete(&mut self, page: PageId) -> KvResult<()>{
        Ok(())
    }



}