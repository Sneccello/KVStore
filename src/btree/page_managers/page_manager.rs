use std::collections::HashMap;
use crate::btree::btree_node::BTreeNode;
use crate::btree::common::PageId;
use crate::errors::KvResult;

pub trait PageManager : Send{
    fn get_node(&self, page: PageId) -> KvResult<&BTreeNode>;
    fn get_node_mut(&mut self, page: PageId) -> KvResult<&mut BTreeNode>;

    fn get_three_mut(&mut self, a: PageId, b: PageId, c: PageId) -> KvResult<(&mut BTreeNode, &mut BTreeNode, &mut BTreeNode)>;

    fn alloc_node(&mut self, node: BTreeNode) -> PageId;

    fn get_pages(&self) -> &HashMap<PageId, BTreeNode>;

    fn delete(&mut self, page: PageId) -> KvResult<()>;

    fn sync(&mut self) -> KvResult<()>;
}

