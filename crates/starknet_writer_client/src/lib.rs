//! Client implementation for [`starknet`] gateway.
//!
//! [`starknet`]: https://starknet.io/

use crate::objects::error::ClientError;
use crate::objects::response::AddTransactionResponse;
use crate::objects::transaction::Transaction;

pub mod objects;

/// A trait defining the methods that the Starknet writer client should support.
#[async_trait]
pub trait StarknetClientTrait {
    /// Add a transaction to Starknet.
    async fn add_transaction(&self, tx: Transaction)
    -> Result<AddTransactionResponse, ClientError>;
}
