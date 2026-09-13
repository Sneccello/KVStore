use async_trait::async_trait;
use crate::errors::KvResult;
#[async_trait]
pub trait StorageEngine: Send + Sync{
    async fn set(&self, key: &[u8], value: &[u8]) -> KvResult<()>;
    fn get(&self, key: &[u8]) -> KvResult<Option<Vec<u8>>>;
    async fn delete(&self, key: &[u8]) -> KvResult<()>;

    fn sync(& self) -> KvResult<()>;
}