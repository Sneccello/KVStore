use kv_store::btree::btree_node::{BTreeNode, StorageMeta};
use crate::common::utils::new_tree;

mod common;

const EXPECTED_HEADER_SIZE_LEAF: usize = 16;
const EXPECTED_HEADER_SIZE_INTERNAL: usize = 16;
#[test]
fn test_header_sizes_should_be_constants(){
    //same header for now
    assert_eq!(size_of::<StorageMeta>(), EXPECTED_HEADER_SIZE_LEAF);
    assert_eq!(size_of::<StorageMeta>(), EXPECTED_HEADER_SIZE_INTERNAL);

}

#[test]
fn test_multilevel_internal_nodes_are_created(){
    let mut tree = new_tree(64); //48 bytes left


    tree.set(b"my16longlongkey0", b"val0").unwrap();
    tree.set(b"my16longlongkey0", b"val0").unwrap();
    tree.set(b"my16longlongkey0", b"val0").unwrap();
    assert_eq!(tree.page_manager.get_pages().len(), 1);
    tree.set(b"my20longverylongkey2", b"val2").unwrap();
    assert_eq!(tree.page_manager.get_pages().len(), 3);
    tree.set(b"my20longverylongkey3", b"val3").unwrap();
    assert_eq!(tree.page_manager.get_pages().len(), 6);
}