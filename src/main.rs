mod errors;
mod engine;
mod btree;

use btree::page_managers::page_manager::PageManager;
use btree::page_managers::persistent_page_manager::PersistentPageManager;

const PAGE_SIZE: usize = 64;
fn main() {
    println!("Hello, world!");
    let page_manager = PersistentPageManager::new_with_temp_file(96);

}
