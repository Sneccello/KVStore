use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

use crate::btree::btree_node::BTreeNode;
use crate::errors::{KvError, KvResult};

pub fn write_node(file: &mut File, offset: u64, node: &BTreeNode) -> KvResult<()> {
    file.seek(SeekFrom::Start(offset)).map_err(|e| KvError::IoError(e.to_string()))?;


    let bytes = bincode::serialize(node).map_err(|e| KvError::IoError(e.to_string()))?;

    let len = bytes.len() as u16;
    file.write_all(&len.to_le_bytes()).map_err(|e| KvError::IoError(e.to_string()))?;

    file.write_all(&bytes).map_err(|e| KvError::IoError(e.to_string()))?;

    Ok(())
}

pub fn read_node(file: &mut File, offset: u64) -> KvResult<BTreeNode> {

    file.seek(SeekFrom::Start(offset)).map_err(|e| KvError::IoError(e.to_string()))?;


    let mut len_buf = [0u8; 2];
    file.read_exact(&mut len_buf).map_err(|e| KvError::IoError(e.to_string()))?;
    let len = u16::from_le_bytes(len_buf) as usize;


    let mut bytes = vec![0u8; len];
    file.read_exact(&mut bytes).map_err(|e| KvError::IoError(e.to_string()))?;


    let node: BTreeNode = bincode::deserialize(&bytes).map_err(|e| KvError::IoError(e.to_string()))?;

    Ok(node)
}


#[cfg(test)]

mod tests {
    use tempfile::tempfile;
    use crate::btree::btree_node::BTreeNode;
    use crate::btree::page_managers::file_utils::{read_node, write_node};
    use crate::btree::internal_node::InternalNode;
    use crate::btree::leaf_node::LeafNode;

    #[test]
    pub fn test_serialization_empty_leaf_node(){

        let mut file = tempfile().unwrap();
        let offset = 0;

        let written_node = BTreeNode::Leaf(LeafNode::new());

        write_node(&mut file, offset, &written_node).unwrap();

        let read_node = read_node(&mut file, offset).unwrap();

        assert_eq!(written_node.is_leaf(), read_node.is_leaf());
        assert_eq!(written_node.as_leaf().keys, read_node.as_leaf().keys);
        assert_eq!(written_node.as_leaf().values, read_node.as_leaf().values);
        assert_eq!(written_node.as_leaf().header, read_node.as_leaf().header);
    }

    #[test]
    pub fn test_serialization_empty_internal_node(){

        let mut file = tempfile().unwrap();
        let offset = 0;

        let written_node = BTreeNode::Internal(InternalNode::new());

        write_node(&mut file, offset, &written_node).unwrap();

        let read_node = read_node(&mut file, offset).unwrap();

        assert_eq!(written_node.is_leaf(), read_node.is_leaf());
        assert_eq!(written_node.as_internal().keys, read_node.as_internal().keys);
        assert_eq!(written_node.as_internal().children, read_node.as_internal().children);
        assert_eq!(written_node.as_internal().header, read_node.as_internal().header);
    }


    #[test]
    pub fn test_serialization_non_empty_leaf_node(){

        let mut file = tempfile().unwrap();
        let offset = 0;

        let mut written_node = BTreeNode::Leaf(LeafNode::new());
        written_node.as_leaf_mut().push_lasts(b"hello".to_vec(), b"world".to_vec());
        written_node.as_leaf_mut().push_lasts(b"hello2".to_vec(), b"world2".to_vec());

        write_node(&mut file, offset, &written_node).unwrap();

        let read_node = read_node(&mut file, offset).unwrap();

        assert_eq!(written_node.is_leaf(), read_node.is_leaf());
        assert_eq!(written_node.as_leaf().keys, read_node.as_leaf().keys);
        assert_eq!(written_node.as_leaf().values, read_node.as_leaf().values);
        assert_eq!(written_node.as_leaf().header, read_node.as_leaf().header);
    }

    #[test]
    pub fn test_serialization_non_empty_internal_node(){

        let mut file = tempfile().unwrap();
        let offset = 0;

        let mut written_node = BTreeNode::Internal(InternalNode::new());
        written_node.as_internal_mut().push_lasts(b"hello".to_vec(), 0u64);
        written_node.as_internal_mut().push_lasts(b"hello2".to_vec(), 1u64);
        written_node.as_internal_mut().push_child(2u64);

        write_node(&mut file, offset, &written_node).unwrap();

        let read_node = read_node(&mut file, offset).unwrap();

        assert_eq!(written_node.is_leaf(), read_node.is_leaf());
        assert_eq!(written_node.as_internal().keys, read_node.as_internal().keys);
        assert_eq!(written_node.as_internal().children, read_node.as_internal().children);
        assert_eq!(written_node.as_internal().header, read_node.as_internal().header);
    }


}