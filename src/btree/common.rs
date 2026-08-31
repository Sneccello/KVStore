use std::time::{SystemTime, UNIX_EPOCH};

pub type PageId = u64;
pub const NULL_PAGE_ID: u64 = u64::MAX;

pub const PAGE_SIZE_PREFIX_BYTES : u16 = 2;


pub fn binary_search_key(key: &[u8], vector: &Vec<Vec<u8>>) -> Result<usize, usize> {
    vector.binary_search_by(
        |k| k.as_slice().cmp(key)
    )
}

pub fn get_unix_nano() -> u128{
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos()
}