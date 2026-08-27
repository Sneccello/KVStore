use crate::errors::KvResult;
pub trait StorageEngine: Send + Sync{
    fn set(&self, key: &[u8], value: &[u8]) -> KvResult<()>;
    fn get(&self, key: &[u8]) -> KvResult<Option<Vec<u8>>>;
    fn delete(&self, key: &[u8]) -> KvResult<()>;

    fn sync(& self) -> KvResult<()>;
}