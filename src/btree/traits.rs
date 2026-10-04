use async_trait::async_trait;
use crate::btree::BTree;
use crate::btree::btree::OperationType;
use crate::btree::btree_node::BTreeNode;
use crate::btree::common::{get_unix_nano, PageId};
use crate::btree::page_managers::persistent_page_manager::LogicalWalRecord;
use crate::engine::StorageEngine;
use crate::errors::KvResult;

#[async_trait]
impl StorageEngine for BTree {
    async fn set(&self, key: &[u8], value: &[u8]) -> KvResult<()> {
        let start = std::time::Instant::now();
        let transaction_id = get_unix_nano();
        self.set_operation(key, value).await?;
        self.page_manager.add_wal_record(
            LogicalWalRecord{
                operation_type: OperationType::Delete,
                transaction_id,
                key: key.to_vec(),
                value: value.to_vec(),
            }
        ).await?;
        self.log_operation(OperationType::Set, start.elapsed().as_nanos());
        Ok(())
    }

    fn get(&self, key: &[u8]) -> KvResult<Option<Vec<u8>>> {
        self.get_operation(key)
    }

    async fn delete(&self, key: &[u8]) -> KvResult<()> {
        let start = std::time::Instant::now();
        let transaction_id = get_unix_nano();
        self.delete_operation(key).await?;
        self.page_manager.add_wal_record(
            LogicalWalRecord{
                operation_type: OperationType::Delete,
                transaction_id,
                key: key.to_vec(),
                value: Vec::new(),
            }
        ).await?;
        self.log_operation(OperationType::Delete, start.elapsed().as_nanos());
        Ok(())
    }

    fn sync(&self) -> KvResult<()> {
        self.page_manager.sync()
    }
}

impl std::fmt::Display for BTree {

    fn fmt(&self, _f: &mut std::fmt::Formatter) -> std::fmt::Result {
        //TODO breaks on last leaves
        let multiplier = 2;
        let dash = "_".repeat(multiplier);
        let space = " ".repeat(multiplier);

        let current = self.root.read().unwrap().clone();
        let mut q = vec!((current, 0, true));
        while ! q.is_empty() {
            let (current, depth, is_last_child) = q.pop().unwrap();
            //println!("visiting node {}", current);
            let node_lock = self.page_manager.get_node(current).unwrap();
            let mut node = node_lock.write().unwrap();

            if depth == 0 {
                println!("{}", current);
            }else{
                let repr = format!("[{current}]");
                let ancestors = format!("|{space}").repeat(depth-1);
                let last = format!("|{dash}").repeat(1);
                let structure = format!("{ancestors}{last}{repr}");
                println!("{structure}");
            }
            match &mut *node{
                BTreeNode::Internal(node) => {
                    let (_, children) = node.get_key_children();
                    let last_child = children.first().unwrap();
                    for page in children.iter(){
                        q.push((page.clone(), depth + 1, page == last_child));
                    }
                },
                BTreeNode::Leaf(leaf) => {
                    for (key, value) in leaf.keys.iter().zip(&leaf.values){
                        let k = String::from_utf8(key.to_vec()).unwrap();
                        let v = String::from_utf8(value.to_vec()).unwrap();
                        let repr = format!("{}->{}", k, v);
                        let ancestors = if depth<=1 || ! is_last_child
                        {
                            format!("|{}", space).repeat(depth)
                        }else{
                            format!("|{}", space).repeat(depth-1) + format!("{space}{space}").as_str()
                        };
                        let last = format!("|{}", dash).repeat(1);
                        let structure = format!("{ancestors}{last}{repr}");

                        println!("{structure}");
                    }

                }
            }
        }
        Ok(())
    }
}

pub trait SerializedSize {
    fn byte_size(&self) -> usize;
}

impl SerializedSize for PageId {
    fn byte_size(&self) -> usize {
        size_of::<PageId>()
    }
}

impl SerializedSize for Vec<u8> {
    fn byte_size(&self) -> usize {
        size_of::<u64>() + self.len()
    }
}

impl SerializedSize for &[u8] {
    fn byte_size(&self) -> usize {
        //we say that storing a serialized byte array is the same as storing its length + bytes.
        // similar to vector
        size_of::<u64>() + self.len()
    }
}