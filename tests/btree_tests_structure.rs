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