mod errors;
mod engine;
mod btree;

use crate::btree::page_manager::{PageManager, PersistentPageManager};
use crate::btree::test_utils::get_test_tree;

const PAGE_SIZE: usize = 64;
fn main() {
    println!("Hello, world!");
    let page_manager = PersistentPageManager::new();
    let tree = get_test_tree();

    println!("{}", tree)
}
