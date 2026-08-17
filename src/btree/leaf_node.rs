use serde::{Deserialize, Serialize};
use crate::btree::btree_node::StorageMeta;
use crate::btree::traits::SerializedSize;
use crate::btree::common::binary_search_key;
use crate::errors::KvError::KeyNotFound;
use crate::errors::KvResult;

#[derive(Debug, Serialize, Deserialize)]
pub struct LeafNode{
    pub header: StorageMeta,
    pub keys: Vec<Vec<u8>>,
    pub values: Vec<Vec<u8>>,
}


impl LeafNode{
    pub fn new() -> Self {
        Self{
            header: StorageMeta::new(),
            keys: Vec::new(),
            values: Vec::new(),
        }
    }


    pub fn set_key_value(&mut self, key: &[u8], value: &[u8]){
        //TODO take ownership?
        let index = binary_search_key(&key, &self.keys);
        match index {
            Ok(idx) => {
                let new_size = value.byte_size();
                let old_value = std::mem::replace(&mut self.values[idx], value.to_vec());
                let old_size = old_value.byte_size();
                if new_size >= old_size {
                    self.header.items_total_size += new_size - old_size;
                } else {
                    self.header.items_total_size -= old_size - new_size;
                }
            },
            Err(idx) => {
                self.insert_key_value(idx, key.to_vec(), idx, value.to_vec());
            }
        }
    }
    pub fn get_value_by_key(&self, key: &[u8]) -> Option<&Vec<u8>>{
        let idx = binary_search_key(&key, &self.keys);
        match idx{
            Ok(idx) => Some(&self.values[idx]),
            Err(_) => None
        }
    }

    pub fn split_off(&mut self, key_index: usize) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {

        let keys = self.keys.split_off(key_index);
        let values = self.values.split_off(key_index);

        let mut key_size_diff = 0;
        for key in &keys {
            key_size_diff += key.byte_size();
        }

        let mut values_size_diff = 0;
        for value in &values {
            values_size_diff += value.byte_size();
        }
        self.header.keys_total_size -= key_size_diff as u16;
        self.header.items_total_size -= values_size_diff as u16;

        (keys, values)
    }

    pub fn append(&mut self, keys: &mut Vec<Vec<u8>>, values: &mut Vec<Vec<u8>>) {
        if keys.len().abs_diff(values.len()) > 1 {
            panic!("Key and children differ too much in length");
        }

        let mut keys_size = 0;
        for key in keys.iter() {
            keys_size += key.byte_size();
        }

        let mut values_size = 0;
        for value in values.iter() {
            values_size += value.byte_size();
        }


        self.header.keys_total_size += keys_size as u16;
        self.header.items_total_size += values_size as u16;

        self.keys.append(keys);
        self.values.append(values);
    }


    pub fn get_keys(&self) -> &Vec<Vec<u8>>{
        &self.keys
    }

    fn insert_key_value(&mut self, key_index: usize, key: Vec<u8>, value_index: usize, value: Vec<u8>) {
        if value_index.abs_diff(key_index) > 1{
            panic!("Cannot insert value with a key that is not theirs")
        }

        self.header.keys_total_size += key.byte_size();
        self.header.items_total_size += value.byte_size();
        self.keys.insert(key_index, key);
        self.values.insert(value_index, value);
    }
    fn remove_key_value(&mut self, key_index: usize, value_index: usize) -> (Vec<u8>, Vec<u8>) {
        if value_index.abs_diff(key_index) > 1{
            panic!("Cannot remove value with a key that is not theirs")
        }

        let key = self.keys.remove(key_index);
        let value = self.values.remove(value_index);
        self.header.keys_total_size -= key.byte_size();
        self.header.items_total_size -= value.byte_size();
        (key, value)
    }


    pub fn delete_key(&mut self, key: &[u8]) -> KvResult<()>{
        match binary_search_key(key, &self.keys){
            Ok(idx) => {
                _ = self.remove_key_value(idx, idx);
                Ok(())
            }
            Err(_) => {
                Err(KeyNotFound(key.to_vec()))
            }
        }
    }

    pub fn push_firsts(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.insert_key_value(0, key, 0, value);
    }

    pub fn push_lasts(&mut self, key: Vec<u8>, value: Vec<u8>) {
        let key_count = self.keys.len();
        self.insert_key_value(key_count, key, key_count, value);
    }

    pub fn remove_lasts(&mut self) -> (Vec<u8>, Vec<u8>) {
        let key_count = self.keys.len();
        self.remove_key_value(key_count-1, key_count-1)
    }

    pub fn remove_firsts(&mut self) -> (Vec<u8>, Vec<u8>) {
        self.remove_key_value(0,0)
    }

    pub fn get_key_values_mut(&mut self) -> (&mut Vec<Vec<u8>>, &mut Vec<Vec<u8>>){
        (&mut self.keys, &mut self.values)
    }

    pub fn get_key_value_by_index(&self, index: usize) -> (&Vec<u8>, &Vec<u8>) {
        (&self.keys[index], &self.values[index])
    }
}



