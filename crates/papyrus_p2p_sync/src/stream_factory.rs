use std::cmp::min;
use std::pin::Pin;
use std::time::Duration;

use async_stream::stream;
use futures::channel::mpsc::Sender;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{SinkExt, Stream, StreamExt};
use papyrus_network::{DataType, Direction, Query};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::BlockNumber;
use tracing::{debug, info};

use crate::{P2PSyncError, STEP};

pub(crate) trait BlockData: Send {
    fn write_to_storage(
        // This is Box<Self> in order to allow using it with `Box<dyn BlockData>`.
        self: Box<Self>,
        storage_writer: &mut StorageWriter,
    ) -> Result<(), StorageError>;
}

pub(crate) enum BlockNumberLimit {
    Unlimited,
    HeaderMarker,
    // TODO(shahak): Add variant for state diff marker once we support classes sync.
}

pub(crate) trait DataStreamFactory {
    type InputFromNetwork: Send + 'static;
    type Output: BlockData + 'static;

    const DATA_TYPE: DataType;
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit;

    // Async functions in trait don't work well with argument references
    fn parse_data_for_block<'a>(
        data_receiver: &'a mut Pin<Box<dyn Stream<Item = Option<Self::InputFromNetwork>> + Send>>,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, P2PSyncError>>;

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError>;

    fn create_stream(
        mut data_receiver: Pin<Box<dyn Stream<Item = Option<Self::InputFromNetwork>> + Send>>,
        mut query_sender: Sender<Query>,
        storage_reader: StorageReader,
        wait_period_for_new_data: Duration,
        num_blocks_per_query: usize,
        stop_sync_at_block_number: Option<BlockNumber>,
    ) -> BoxStream<'static, Result<Box<dyn BlockData>, P2PSyncError>> {
        stream! {
            let mut current_block_number = Self::get_start_block_number(&storage_reader)?;
            'send_query_and_parse_responses: loop {
                let limit = match Self::BLOCK_NUMBER_LIMIT {
                    BlockNumberLimit::Unlimited => num_blocks_per_query,
                    BlockNumberLimit::HeaderMarker => {
                        let last_block_number = storage_reader.begin_ro_txn()?.get_header_marker()?;
                        let limit = min(
                            usize::try_from(last_block_number.0 - current_block_number.0)
                                .expect("failed converting u64 to usize"),
                            num_blocks_per_query,
                        );
                        if limit == 0 {
                            debug!("{:?} sync is waiting for a new header", Self::DATA_TYPE);
                            tokio::time::sleep(wait_period_for_new_data).await;
                            continue;
                        }
                        limit
                    }
                };
                let end_block_number = current_block_number.0
                    + u64::try_from(limit)
                        .expect("Failed converting usize to u64");
                debug!(
                    "Downloading {:?} for blocks [{}, {})",
                    Self::DATA_TYPE,
                    current_block_number.0,
                    end_block_number,
                );
                query_sender
                    .send(Query {
                        start_block: current_block_number,
                        direction: Direction::Forward,
                        limit,
                        step: STEP,
                        data_type: Self::DATA_TYPE,
                    })
                    .await?;

                while current_block_number.0 < end_block_number {
                    match Self::parse_data_for_block(
                        &mut data_receiver, current_block_number, &storage_reader
                    ).await? {
                        Some(output) => yield Ok(Box::<dyn BlockData>::from(Box::new(output))),
                        None => {
                            debug!(
                                "Query for {:?} returned with partial data. Waiting {:?} before \
                                 sending another query.",
                                Self::DATA_TYPE,
                                wait_period_for_new_data
                            );
                            tokio::time::sleep(wait_period_for_new_data).await;
                            continue 'send_query_and_parse_responses;
                        }
                    }
                    info!("Added {:?} for block {}.", Self::DATA_TYPE, current_block_number);
                    current_block_number = current_block_number.unchecked_next();
                    if stop_sync_at_block_number.is_some_and(|stop_sync_at_block_number| {
                        current_block_number >= stop_sync_at_block_number
                    }) {
                        return;
                    }
                }

                // Consume the None message signaling the end of the query.
                match data_receiver.next().await {
                    Some(None) => {
                        debug!("Query sent to network for {:?} finished", Self::DATA_TYPE);
                    },
                    Some(Some(_)) => Err(P2PSyncError::TooManyResponses)?,
                    None => Err(P2PSyncError::ReceiverChannelTerminated {
                        data_type: Self::DATA_TYPE
                    })?,
                }
            }
        }
        .boxed()
    }
}
