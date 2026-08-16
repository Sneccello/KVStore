use std::collections::HashMap;

use crate::btree::btree_node::{BTreeNode};
use crate::btree::common::PageId;
use crate::errors::{KvError, KvResult};

pub struct PageManager{
    page_id: PageId,
    pages: HashMap<PageId, BTreeNode>
}

impl PageManager {

    pub fn new() -> PageManager {
        Self{
            page_id: 0,
            pages: HashMap::new()
        }
    }

    pub fn get_node(&self, page: PageId) -> KvResult<&BTreeNode>{
        self.pages.get(&page).ok_or_else(|| KvError::PageNotFound(page.clone()))
    }

    pub fn get_node_mut(&mut self, page: PageId) -> KvResult<&mut BTreeNode>{
        self.pages.get_mut(&page).ok_or_else(|| KvError::PageNotFound(page.clone()))
    }


    pub fn get_two_mut(&mut self, a: PageId, b: PageId) -> KvResult<(&mut BTreeNode, &mut BTreeNode)>{
        if a == b{
            return Err(KvError::InvalidPageRequest(vec!(a.clone(), b.clone())));
        }

        let node_a = self.pages.get_mut(&a).ok_or_else(|| KvError::PageNotFound(a.clone()))? as *mut BTreeNode;
        let node_b = self.pages.get_mut(&b).ok_or_else(|| KvError::PageNotFound(a.clone()))?;

        // SAFETY: We checked id_a != id_b, so node_a and node_b
        // point to completely non-overlapping memory.
        unsafe {
            Ok((&mut *node_a, node_b))
        }
    }

    pub fn get_three_mut(&mut self, a: PageId, b: PageId, c: PageId) -> KvResult<(&mut BTreeNode, &mut BTreeNode, &mut BTreeNode)>{
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

    pub fn alloc_node(&mut self, node: BTreeNode) -> PageId {
        let id = self.page_id;
        self.page_id += 1;
        self.pages.insert(id, node);
        id
    }

    pub fn get_pages(&self) -> &HashMap<PageId, BTreeNode>{
        &self.pages
    }

    pub fn delete(&mut self, page: PageId) -> KvResult<()>{
        Ok(())
    }



}