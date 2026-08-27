use serde::{Deserialize, Serialize};
use crate::btree::btree_node::StorageMeta;
use crate::btree::traits::SerializedSize;
use crate::btree::common::{binary_search_key, PageId};


#[derive(Debug, Serialize, Deserialize)]
pub struct InternalNode{
    pub header: StorageMeta,
    pub keys: Vec<Vec<u8>>,
    pub children: Vec<PageId>,
}

impl InternalNode{

    pub fn new() -> Self {
        InternalNode{
            header: StorageMeta::new(),
            keys: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn route_key_to_child(&self, key: &[u8]) -> PageId {
        let idx = binary_search_key(&key, &self.keys).unwrap_or_else(|idx| idx);
        self.get_child_by_index(idx)
    }

    pub fn route_key_to_index(&self, key: &[u8]) -> usize {
        binary_search_key(&key, &self.keys).unwrap_or_else(|idx| idx)
    }

    pub fn get_child_by_index(&self, index: usize) -> PageId{
        self.children[index]
    }


    pub fn split_off(&mut self, key_index: usize) -> (Vec<Vec<u8>>, Vec<PageId>) {

        let keys = self.keys.split_off(key_index);
        let children = self.children.split_off(key_index);

        let mut size_diff = 0;
        for key in &keys {
            size_diff += key.byte_size();
        }
        self.header.keys_total_size -= size_diff as u16;
        self.header.items_total_size -= (children.len() * size_of::<PageId>()) as u16;

        (keys, children)
    }

    pub fn append(&mut self, keys: &mut Vec<Vec<u8>>, children: &mut Vec<PageId>) {
        if keys.len().abs_diff(children.len()) > 1 {
            panic!("Key and children differ too much in length");
        }

        let mut size_diff = 0;
        for key in keys.iter() {
            size_diff += key.byte_size();
        }

        self.header.keys_total_size += size_diff as u16;
        self.header.items_total_size += (children.len() * size_of::<PageId>()) as u16;

        self.keys.append(keys);
        self.children.append(children);
    }

    pub fn get_keys(&self) -> &Vec<Vec<u8>>{
        &self.keys
    }

    pub fn push_child(&mut self, page_id: PageId) {
        self.children.push(page_id);
        self.header.items_total_size += page_id.byte_size();
    }


    pub fn pop_last_key(&mut self) -> Vec<u8>{
        let key = self.keys.pop().unwrap();
        self.header.keys_total_size -= key.byte_size();
        key
    }

    pub fn insert_key_child(&mut self, key_index: usize, key: Vec<u8>, child_index: usize, child: PageId) {
        if child_index.abs_diff(key_index) > 1{
            panic!("Cannot insert child with a routing key that is not theirs key:{key_index} and child:{child_index}")
        }
        let key_len = key.byte_size();
        self.keys.insert(key_index, key);
        self.children.insert(child_index, child);
        self.header.keys_total_size += key_len;
        self.header.items_total_size += child.byte_size();
    }

    pub fn remove_key_child(&mut self, key_index: usize, child_index: usize) -> (Vec<u8>, PageId) {
        if child_index.abs_diff(key_index) > 1{
            panic!("Cannot remove child with a routing key that is not theirs")
        }
        let key = self.keys.remove(key_index);
        let child = self.children.remove(child_index);
        self.header.keys_total_size -= key.byte_size();
        self.header.items_total_size -= child.byte_size();
        (key, child)
    }


    pub fn push_firsts(&mut self, key: Vec<u8>, child: PageId) {
        self.insert_key_child(0, key, 0, child);
    }

    pub fn push_lasts(&mut self, key: Vec<u8>, child: PageId) {
        let key_count = self.keys.len();
        let child_count = self.children.len();
        self.insert_key_child(key_count, key, child_count, child);
    }

    pub fn remove_lasts(&mut self) -> (Vec<u8>, PageId) {
        let key_count = self.keys.len();
        self.remove_key_child(key_count-1, key_count)
    }

    pub fn remove_firsts(&mut self) -> (Vec<u8>, PageId) {
        self.remove_key_child(0,0)
    }

    pub fn get_key_children_mut(&mut self) -> (&mut Vec<Vec<u8>>, &mut Vec<PageId>){
        (&mut self.keys, &mut self.children)
    }
    pub fn get_key_children(&self) -> (&Vec<Vec<u8>>, &Vec<PageId>){
        (&self.keys, &self.children)
    }

    pub fn overwrite_key(&mut self, index: usize, key: Vec<u8>) {
        let size = key.byte_size();
        let old_key = std::mem::replace(&mut self.keys[index],key);
        if size >= old_key.byte_size() {
            self.header.keys_total_size += size - old_key.byte_size();
        } else {
            self.header.keys_total_size -= old_key.byte_size() - size;
        }
    }

    pub fn overwrite_value(&mut self, index: usize, value: PageId) {
        let size = value.byte_size();
        let old_value = std::mem::replace(&mut self.children[index], value);
        if size >= old_value.byte_size() {
            self.header.items_total_size += size - old_value.byte_size();
        } else {
            self.header.items_total_size -= old_value.byte_size() - size;
        }
    }

}
