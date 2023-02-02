use goose::goose::{GooseUser, TransactionResult};

use crate::gateway_endpoints::*;

pub async fn loadtest_get_block_with_tx_hashes_by_number(
    user: &mut GooseUser,
) -> TransactionResult {
    let _: serde_json::Value = get_block_with_tx_hashes_by_number(user, 1).await?;
    Ok(())
}

pub async fn loadtest_get_block_with_tx_hashes_by_hash(user: &mut GooseUser) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value = get_block_with_tx_hashes_by_hash(
        user,
        "0x1d997fd79d81bb4c30c78d7cb32fb8a59112eeb86347446235cead6194aed07",
    )
    .await?;
    Ok(())
}
