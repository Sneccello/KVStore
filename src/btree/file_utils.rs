use std::fs::File;
use std::io::{Seek, SeekFrom};
use crate::btree::btree_node::BTreeNode;
use crate::btree::common::PageId;
use crate::btree::leaf_node::LeafNode;
use crate::errors::{KvError, KvResult};

pub fn write_leaf_node(file: &mut File, offset: u64, node: &LeafNode) -> KvResult<()> {
    Ok(())  
    /*
    file.seek(SeekFrom::Start(offset));

    let bytes = bincode::serialize(node)
        .map_err(|e| KvError::IoError(e.to_string());

    len

*/
}
