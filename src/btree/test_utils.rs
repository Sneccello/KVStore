use crate::btree::BTree;
use crate::btree::btree_node::BTreeNode;
use crate::btree::common::{PageId, PAGE_SIZE_PREFIX_BYTES};
use crate::btree::internal_node::InternalNode;
use crate::btree::leaf_node::LeafNode;
use crate::btree::page_managers::persistent_page_manager::PersistentPageManager;
use crate::btree::page_managers::page_manager::PageManager;

pub fn new_persistent_page_manager(page_size: u16) -> Box<dyn PageManager> {
    //for now we add the prefix so the tests can focus on the useful page size
    // without the additional meta for node size as a prefix
    Box::new(PersistentPageManager::new_with_temp_file(page_size+PAGE_SIZE_PREFIX_BYTES))
}

pub fn get_empty_leaf_root(page_size: u16) -> BTree {
    let manager = new_persistent_page_manager(page_size);
    BTree::new(manager, page_size)
}

pub fn get_empty_internal_root(page_size: u16) -> BTree {
    let manager = new_persistent_page_manager(page_size);
    let mut tree = BTree::new(manager, page_size);
    tree.page_manager =new_persistent_page_manager(page_size); //get rid of initialized pages above
    let internal_root = BTreeNode::Internal(InternalNode::new());
    let root_page = tree.page_manager.alloc_node(internal_root);
    tree.root = root_page;
    tree
}

pub fn new_internal(manager: &mut Box<dyn PageManager>) -> PageId {
    let node =  InternalNode::new();
    manager.alloc_node(BTreeNode::Internal(node))
}

pub fn new_leaf(manager: &mut Box<dyn PageManager>) -> PageId {
    let node =  LeafNode::new();
    manager.alloc_node(BTreeNode::Leaf(node))
}

pub fn get_test_tree() -> BTree {

    /*
            ____root___
          /      |      \
       i1       i2        i3
     / | \     / | \     / | \
   l1 l2 l3   l1 l2 l3  l4 l5 l6

   each leaf holds 5, 10byte (key=6, value=4) key-values
   total of 9x5=45 key-values

     */
    let page_size = 64;
    let mut manager = new_persistent_page_manager(page_size);

    let root_page_id = new_internal(&mut manager);
    let i1_page = new_internal(&mut manager);
    let i2_page = new_internal(&mut manager);
    let i3_page = new_internal(&mut manager);

    //root has 48 bytes free.
    //root has 2 keys, 3 children leaving 20 bytes free
    //28 bytes is 2x14 byte key values where a key is 6 bytes

    //internal nodes
    for (page, internal_page_idx) in [i1_page, i2_page, i3_page].iter().zip(0..3){

        let mut last_leaf_key = None;
        for leaf_index in 0..3{
            let leaf_page = new_leaf(&mut manager);
            let node = manager.get_node_mut(page.clone()).unwrap().as_internal_mut();
            node.push_child(leaf_page);

            let child = manager.get_node_mut(leaf_page).unwrap().as_leaf_mut();
            for kv_index in 0..5{
                let entry_id = internal_page_idx*15 + leaf_index*5 + kv_index;
                let key = format!("key{:0>3}", entry_id).as_bytes().to_vec();
                let value = format!("val{:0>3}", entry_id).as_bytes().to_vec();

                last_leaf_key = Some(key.clone());
                child.push_lasts(key, value);


            }

            if leaf_index < 2{
                let last_key = child.keys.last().unwrap().clone();
                let node = manager.get_node_mut(page.clone()).unwrap().as_internal_mut();
                node.keys.push(last_key);
            }
        }

        let root = manager.get_node_mut(root_page_id).unwrap().as_internal_mut();
        if internal_page_idx < 2{

            root.push_lasts(last_leaf_key.unwrap(), page.clone());
        }else{
            root.push_child(page.clone());
        }


    }

    let root = manager.get_node(root_page_id).unwrap().as_internal();
    assert_eq!(root.keys, &[b"key014", b"key029"]);

    let i1 = manager.get_node(root.children[0]).unwrap().as_internal();
    assert_eq!(i1.keys, &[b"key004", b"key009"]);

    let i2 = manager.get_node(root.children[1]).unwrap().as_internal();
    assert_eq!(i2.keys, &[b"key019", b"key024"]);

    let i3 = manager.get_node(root.children[2]).unwrap().as_internal();
    assert_eq!(i3.keys, &[b"key034", b"key039"]);

    let mut tree = BTree::new(manager, page_size);
    tree.root = root_page_id;
    tree

}