
use std::io;
use crate::btree::common::PageId;

#[derive(Debug)]
pub enum KvError{
    KeyNotFound(Vec<u8>),
    IoError(String),
    CorruptedData(String),
    PageNotFound(PageId),
    InvalidPageRequest(Vec<PageId>),
    TreeLogicError(String),
}

impl std::error::Error for KvError{}

impl std::fmt::Display for KvError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            KvError::KeyNotFound(key) => write!(f, "Key not found, {:?}", key),
            KvError::IoError(err) => write!(f, "IO error: {}", err),
            KvError::CorruptedData(msg) => write!(f, "Corrupted data {}", msg),
            KvError::PageNotFound(page_id) => write!(f, "Page not found {}", page_id),
            KvError::InvalidPageRequest(page_vec) => write!(f, "Invalid page request {:?}", page_vec),
            KvError::TreeLogicError(msg) => write!(f, "Logic error: {}", msg),
        }
    }
}
pub type KvResult<E> = std::result::Result<E, KvError>;
