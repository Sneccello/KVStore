
pub type PageId = u64;
pub const NULL_PAGE_ID: u64 = u64::MAX;

#[repr(u8)]
pub enum NodeType {
    Internal = 1,
    Leaf = 2,
}



pub fn binary_search_key(key: &[u8], vector: &Vec<Vec<u8>>) -> Result<usize, usize> {
    vector.binary_search_by(
        |k| k.as_slice().cmp(key)
    )
}