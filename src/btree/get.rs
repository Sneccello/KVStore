use crate::btree::BTree;
use crate::btree::btree_node::BTreeNode;
use crate::errors::KvError::LockError;
use crate::errors::KvResult;

impl BTree{
    pub fn get(&self, key: &[u8]) -> KvResult<Option<Vec<u8>>> {

        let guard = self.root.read().map_err(|_| LockError())?; //TODO guard
        let mut current_page = *guard;
        loop {
            let node = self.page_manager.get_node(current_page)?;
            let guard = node.read().map_err(|e| LockError())?;

            match &*guard{
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
    pub fn recursive_get(&self, key: &[u8]) -> KvResult<Option<Vec<u8>>> {

        let guard = self.root.read().map_err(|_| LockError())?; //TODO guard
        let mut current_page = *guard;
        loop {
            let node = self.page_manager.get_node(current_page)?;
            let guard = node.read().map_err(|e| LockError())?;

            match &*guard{
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