use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::btree::btree_node::{BTreeNode};
use crate::btree::common::PageId;
use crate::btree::file_utils::write_leaf_node;
use crate::errors::{KvError, KvResult};

pub trait PageManager {
    fn get_node(&self, page: PageId) -> KvResult<&BTreeNode>;
    fn get_node_mut(&mut self, page: PageId) -> KvResult<&mut BTreeNode>;

    fn get_three_mut(&mut self, a: PageId, b: PageId, c: PageId) -> KvResult<(&mut BTreeNode, &mut BTreeNode, &mut BTreeNode)>;

    fn alloc_node(&mut self, node: BTreeNode) -> PageId;

    fn get_pages(&self) -> &HashMap<PageId, BTreeNode>;

    fn delete(&mut self, page: PageId) -> KvResult<()>;

    fn sync(&mut self) -> KvResult<()>;
}

pub struct PersistentPageManager{
    next_free_page_id: PageId,
    pages: HashMap<PageId, BTreeNode>,
    free_list: BinaryHeap<Reverse<PageId>>,
    dirty_pages: Vec<PageId>,
    block_size: u16,
}

impl PageManager for PersistentPageManager{

    fn get_node(&self, page: PageId) -> KvResult<&BTreeNode>{
        self.pages.get(&page).ok_or_else(|| KvError::PageNotFound(page.clone()))
    }

    fn get_node_mut(&mut self, page: PageId) -> KvResult<&mut BTreeNode> {
        if !self.pages.contains_key(&page) {
            return Err(KvError::PageNotFound(page.clone()));
        }
        self.dirty_pages.push(page);
        Ok(self.pages.get_mut(&page).unwrap())
    }


    fn get_three_mut(&mut self, a: PageId, b: PageId, c: PageId) -> KvResult<(&mut BTreeNode, &mut BTreeNode, &mut BTreeNode)>{
        if a==b || b==c || c==a{
            return Err(KvError::TreeLogicError("Page ids are the same for different nodes".to_string())); //TODO
        }

        let node_a = self.pages.get_mut(&a).ok_or_else(|| KvError::PageNotFound(a.clone()))? as *mut BTreeNode;
        let node_b = self.pages.get_mut(&b).ok_or_else(|| KvError::PageNotFound(b.clone()))? as *mut BTreeNode;
        let node_c = self.pages.get_mut(&c).ok_or_else(|| KvError::PageNotFound(c.clone()))?;
        self.dirty_pages.push(a);
        self.dirty_pages.push(b);
        self.dirty_pages.push(c);
        unsafe{
            Ok((&mut *node_a, &mut *node_b, node_c))
        }
    }

    fn alloc_node(&mut self, node: BTreeNode) -> PageId {
        let id = match self.free_list.pop(){
            Some(Reverse(id)) => id,
            None => {
                let id = self.next_free_page_id;
                self.next_free_page_id+=1;
                id
            }
        };
        self.pages.insert(id, node);
        self.dirty_pages.push(id);
        id
    }

    fn get_pages(&self) -> &HashMap<PageId, BTreeNode>{
        &self.pages
    }

    fn delete(&mut self, page: PageId) -> KvResult<()>{
        self.free_list.push(Reverse(page));
        Ok(())
    }

    fn sync(&mut self) -> KvResult<()>{

        let collection : Vec<PageId> = self.dirty_pages.drain(..).collect();
        for page in collection{
            let offset = self.get_block_offset(page);
            match self.pages.get(&page){
                Some(BTreeNode::Internal(node)) => {
                    //write_internal_node(page, node)?
                },
                Some(BTreeNode::Leaf(node)) => {
                    //write_leaf_node(page, node)?
                }
                None => {
                    return Err(KvError::PageNotFound(page));
                }
            }

        }

        Ok(())
    }

}


impl PersistentPageManager{

    pub fn new() -> PersistentPageManager {
        Self{
            next_free_page_id: 0,
            pages: HashMap::new(),
            free_list: BinaryHeap::new(),
            dirty_pages: Vec::new(),
            block_size: 1<<12, //TODO param
        }
    }

    fn get_block_offset(&self, page_id: PageId) -> u64{
        (self.block_size as u64) * (page_id as u64)
    }
}


