use serde::{Deserialize, Serialize};
use crate::btree::common::{PageId, NULL_PAGE_ID};
use crate::btree::internal_node::InternalNode;
use crate::btree::leaf_node::LeafNode;


#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[repr(C)]
pub struct StorageMeta{
    pub next: PageId,
    pub items_total_size: u16,
    pub keys_total_size: u16,
    pub _padding: [u8; 4],
}


impl StorageMeta{
    pub fn new() -> Self{
        Self{
            items_total_size: 0,
            keys_total_size: 0,
            next: NULL_PAGE_ID,
            _padding: [0; 4],
        }
    }

    pub fn total_size_bytes(&self) -> u16 {

        self.items_total_size
            + self.keys_total_size
            + (size_of::<StorageMeta>() as u16)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BTreeNode {
    Internal(InternalNode),
    Leaf(LeafNode)
}

impl BTreeNode{

    pub fn total_size_bytes(&self) -> u16{
        match self {
            BTreeNode::Internal(node) => {
                node.header.total_size_bytes()
            }
            BTreeNode::Leaf(node) => {
                node.header.total_size_bytes()
            }
        }
    }

    pub fn key_count(&self) -> u16{
        match self {
            BTreeNode::Internal(node) => {
                node.get_keys().len() as u16
            }
            BTreeNode::Leaf(node) => {
                node.get_keys().len() as u16
            }
        }
    }

    pub fn is_leaf(&self) -> bool{
        match self {
            BTreeNode::Leaf(_) => {true},
            _ => {false}
        }
    }

    pub fn as_internal(&self) -> &InternalNode {
        match self {
            BTreeNode::Internal(node) => {
                node
            }
            _ => {unreachable!()}
        }
    }

    pub fn as_internal_mut(&mut self) -> &mut InternalNode {
        match self {
            BTreeNode::Internal(node) => {
                node
            }
            _ => {unreachable!()}
        }
    }

    pub fn as_leaf(&self) -> &LeafNode {
        match self {
            BTreeNode::Leaf(node) => {
                node
            }
            _ => {unreachable!()}
        }
    }

    pub fn as_leaf_mut(&mut self) -> &mut LeafNode {
        match self {
            BTreeNode::Leaf(node) => {
                node
            }
            _ => {unreachable!()}
        }
    }
}



