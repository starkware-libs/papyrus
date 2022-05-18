use super::ClientError;

#[async_trait::async_trait]
pub trait CentralApi {
    async fn block_number(&self) -> Result<u32, ClientError>;
}
