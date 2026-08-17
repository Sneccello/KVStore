use kv_store::btree::BTree;
use kv_store::btree::page_manager::PageManager;
use kv_store::btree::test_utils::new_persistent_page_manager;
use kv_store::errors::KvResult;

pub fn insert_keys_values(tree: &mut BTree, keys: &Vec<Vec<u8>>, values: &Vec<Vec<u8>>) {

    for (key, value) in keys.iter().zip(values.iter()){
        tree.set(&key, &value).unwrap();
    }
}

pub fn new_tree(page_size: u16) -> BTree {
    let page_manager = new_persistent_page_manager();
    BTree::new(page_manager, page_size)
}

pub fn shuffle(vector: &mut Vec<Vec<u8>>){
    let seed_val = 42u64; // Change this to whatever seed you want
    let mut seed = seed_val;
    let len = vector.len();

    for i in (1..len).rev() {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = (seed as usize) % (i + 1);
        vector.swap(i, j);
    }
}