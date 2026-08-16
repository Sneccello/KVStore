use crate::errors::KvResult;
pub trait StorageEngine{
    fn set(&mut self, key: &[u8], value: &[u8]) -> KvResult<()>;
    fn get(&self, key: &[u8]) -> KvResult<Option<Vec<u8>>>;
    fn delete(&mut self, key: &[u8]) -> KvResult<()>;

    fn sync(&mut self) -> KvResult<()>;
}