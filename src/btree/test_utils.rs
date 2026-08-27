use std::sync::Arc;
use crate::btree::BTree;
use crate::btree::btree_node::BTreeNode;
use crate::btree::common::{PageId, PAGE_SIZE_PREFIX_BYTES};
use crate::btree::internal_node::InternalNode;
use crate::btree::leaf_node::LeafNode;
use crate::btree::page_managers::persistent_page_manager::PersistentPageManager;
use crate::btree::page_managers::page_manager::PageManager;

pub fn new_persistent_page_manager(page_size: u16) -> Arc<dyn PageManager> {
    //for now we add the prefix so the tests can focus on the useful page size
    // without the additional meta for node size as a prefix
    Arc::new(PersistentPageManager::new_with_temp_file(page_size+PAGE_SIZE_PREFIX_BYTES))
}

pub fn get_empty_leaf_root(page_size: u16) -> BTree {
    let manager = new_persistent_page_manager(page_size);
    BTree::new(manager, page_size)
}

pub fn get_empty_internal_root(page_size: u16) -> BTree {
    let manager = new_persistent_page_manager(page_size);
    let mut tree = BTree::new(manager, page_size);
    tree.page_manager.delete(get_root_page(&tree)).unwrap(); //get rid of initialized pages above
    let internal_root = BTreeNode::Internal(InternalNode::new());
    let root_page = tree.page_manager.alloc_node(internal_root).unwrap();
    let mut root_guard = tree.root.write().unwrap();
    *root_guard = root_page;
    drop(root_guard);
    tree
}

pub fn new_internal(manager: &mut Arc<dyn PageManager>) -> PageId {
    let node =  InternalNode::new();
    manager.alloc_node(BTreeNode::Internal(node)).unwrap()
}

pub fn new_leaf(manager: &mut Arc<dyn PageManager>) -> PageId {
    let node =  LeafNode::new();
    manager.alloc_node(BTreeNode::Leaf(node)).unwrap()
}

pub fn get_root_page(tree: &BTree) -> PageId {
    tree.root.read().unwrap().clone()
}