use std::sync::RwLockReadGuard;
use crate::btree::BTree;
use crate::btree::btree::OperationType;
use crate::btree::btree_node::BTreeNode;
use crate::errors::KvError::LockError;
use crate::errors::KvResult;

impl BTree{
    pub fn get(&self, key: &[u8]) -> KvResult<Option<Vec<u8>>> {
        let start = std::time::Instant::now();
        let guard = self.root.read().map_err(|_| LockError())?; //TODO guard
        let root_page = *guard;
        let lock = self.page_manager.get_node(root_page)?;
        let guard = lock.read().map_err(|e| LockError())?;
        let res = self.recursive_get(key, guard);
        self.log_operation(OperationType::Get, start.elapsed().as_nanos());
        res

    }
    fn recursive_get(&self, key: &[u8], guard: RwLockReadGuard<BTreeNode>) -> KvResult<Option<Vec<u8>>> {
        
        match &*guard{
            BTreeNode::Internal(node) => {
                let page = node.route_key_to_child(key);
                let lock = self.page_manager.get_node(page)?;
                let guard = lock.read().map_err(|e| LockError())?;
                self.recursive_get(key, guard)
            }
            BTreeNode::Leaf(node) => {
                let value = node.get_value_by_key(key);
                Ok(value.and_then(|value| Some(value.clone())))
            }
        }

    }
}