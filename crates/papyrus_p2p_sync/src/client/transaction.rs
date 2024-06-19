use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use papyrus_storage::body::{BodyStorageReader, BodyStorageWriter};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::{BlockBody, BlockNumber};
use starknet_api::transaction::{Transaction, TransactionOutput};

use crate::stream_factory::{BlockData, BlockNumberLimit, DataStreamFactory};
use crate::{P2PSyncError, ResponseReceiver, NETWORK_DATA_TIMEOUT};

impl BlockData for (BlockBody, BlockNumber) {
    fn write_to_storage(
        self: Box<Self>,
        storage_writer: &mut StorageWriter,
    ) -> Result<(), StorageError> {
        storage_writer.begin_rw_txn()?.append_body(self.1, self.0)?.commit()
    }
}

pub(crate) struct TransactionStreamFactory;

impl DataStreamFactory<(Transaction, TransactionOutput)> for TransactionStreamFactory {
    type Output = (BlockBody, BlockNumber);

    const TYPE_DESCRIPTION: &'static str = "transactions";
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit = BlockNumberLimit::HeaderMarker;

    fn parse_data_for_block<'a>(
        transactions_receiver: &'a mut ResponseReceiver<(Transaction, TransactionOutput)>,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, P2PSyncError>> {
        async move {
            let mut result = BlockBody::default();
            let mut current_transaction_len = 0;
            let target_transaction_len = storage_reader
                .begin_ro_txn()?
                .get_block_header(block_number)?
                .expect("A header with number lower than the header marker is missing")
                .n_transactions
                .ok_or(P2PSyncError::OldHeaderInStorage {
                    block_number,
                    missing_field: "n_transactions",
                })?;
            while current_transaction_len < target_transaction_len {
                let (maybe_transaction, _report_callback) =
                    tokio::time::timeout(NETWORK_DATA_TIMEOUT, transactions_receiver.next())
                        .await?
                        .ok_or(P2PSyncError::ReceiverChannelTerminated {
                            type_description: Self::TYPE_DESCRIPTION,
                        })?;
                let Some((transaction, transaction_output)) = maybe_transaction?.0 else {
                    if current_transaction_len == 0 {
                        return Ok(None);
                    } else {
                        return Err(P2PSyncError::WrongNumberOfTransactions {
                            expected: target_transaction_len,
                            actual: current_transaction_len,
                        });
                    }
                };
                // TODO(eitan): Add transaction hashes to the block body by reteiving chainid from
                // storage
                result.transactions.push(transaction);
                result.transaction_outputs.push(transaction_output);
                current_transaction_len += 1;
            }
            Ok(Some((result, block_number)))
        }
        .boxed()
    }

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError> {
        storage_reader.begin_ro_txn()?.get_body_marker()
    }
}
