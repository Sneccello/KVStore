use crate::btree::btree_node::{BTreeNode};
use crate::btree::common::{PageId};
use crate::btree::leaf_node::LeafNode;
use crate::errors::KvResult;
use crate::btree::page_managers::page_manager::PageManager;


pub struct BTree{
    pub root: PageId,
    pub node_fat_limit_bytes: u16,
    pub node_thin_limit_bytes: u16, //TODO
    pub page_manager: Box<dyn PageManager>,
}

impl BTree{
    pub fn new(mut page_manager: Box<dyn PageManager>, useful_page_size: u16) -> Self {

        let root = LeafNode::new();
        let root_page = page_manager.alloc_node(BTreeNode::Leaf(root));
        BTree{
            root: root_page,
            node_fat_limit_bytes: useful_page_size,
            node_thin_limit_bytes: useful_page_size / 2,
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



