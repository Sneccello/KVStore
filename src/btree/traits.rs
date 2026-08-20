use crate::btree::BTree;
use crate::btree::btree_node::BTreeNode;
use crate::btree::common::PageId;
use crate::engine::StorageEngine;
use crate::errors::KvResult;

impl StorageEngine for BTree {
    fn set(&mut self, key: &[u8], value: &[u8]) -> KvResult<()> {
        self.set(key, value)
    }

    fn get(&self, key: &[u8]) -> KvResult<Option<Vec<u8>>> {
        self.get(key)
    }

    fn delete(&mut self, key: &[u8]) -> KvResult<()> {
        self.delete(key)
    }

    fn sync(&mut self) -> KvResult<()> {
        self.page_manager.sync()
    }
}

impl std::fmt::Display for BTree {

    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        //TODO breaks on last leaves
        let multiplier = 2;
        let dash = "_".repeat(multiplier);
        let space = " ".repeat(multiplier);

        let current = self.root;
        let mut q = vec!((current, 0, true));
        while ! q.is_empty() {
            let (current, depth, is_last_child) = q.pop().unwrap();
            //println!("visiting node {}", current);
            let node = self.page_manager.get_node(current).unwrap();

            if depth == 0 {
                println!("{}", current);
            }else{
                let repr = format!("[{current}]");
                let ancestors = format!("|{space}").repeat(depth-1);
                let last = format!("|{dash}").repeat(1);
                let structure = format!("{ancestors}{last}{repr}");
                println!("{structure}");
            }
            match node{
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
    fn byte_size(&self) -> u16;
}

impl SerializedSize for PageId {
    fn byte_size(&self) -> u16 {
        size_of::<PageId>() as u16
    }
}

impl SerializedSize for Vec<u8> {
    fn byte_size(&self) -> u16 {
        (size_of::<u64>() + self.len()) as u16
    }
}

impl SerializedSize for &[u8] {
    fn byte_size(&self) -> u16 {
        //we say that storing a serialized byte array is the same as storing its length + bytes.
        // similar to vector
        (size_of::<u64>() + self.len()) as u16
    }
}