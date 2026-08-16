use crate::btree::btree_node::{BTreeNode, StorageMeta};
use crate::btree::traits::ByteSized;
use crate::btree::common::PageId;
use crate::btree::internal_node::InternalNode;
use crate::btree::leaf_node::LeafNode;
use crate::errors::KvResult;
use crate::btree::page_manager::PageManager;


pub struct BTree{
    pub root: PageId,
    pub page_size: usize,
    pub page_size_half: usize, //TODO
    pub page_manager: PageManager,
}

impl BTree{
    pub fn new(mut page_manager: PageManager, page_size: usize) -> Self{

        let root = LeafNode::new();
        let root_page = page_manager.alloc_node(BTreeNode::Leaf(root));
        BTree{
            root: root_page,
            page_size,
            page_size_half: page_size / 2,
            page_manager,

        }
    }

    pub fn get(&self, key: &[u8]) -> KvResult<Option<Vec<u8>>> {

        let mut current_page = self.root;

        loop {
            let node = self.page_manager.get_node(current_page)?;

            match node{
                BTreeNode::Internal(node) => {
                    current_page = node.route_key_to_child(key);
                }
                BTreeNode::Leaf(node) => {
                    return Ok(node.get_value_by_key(key)
                        .and_then(|value| Some(value.clone())))
                }
            }
        }
    }
}


//TODO maybe merge root into leaf


