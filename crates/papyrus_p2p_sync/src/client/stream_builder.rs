use std::cmp::min;
use std::time::Duration;

use async_stream::stream;
use futures::channel::oneshot;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{SinkExt, StreamExt};
use papyrus_network::network_manager::SqmrClientPayload;
use papyrus_protobuf::converters::ProtobufConversionError;
use papyrus_protobuf::sync::{BlockHashOrNumber, DataOrFin, Direction, Query};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::BlockNumber;
use tracing::{debug, info};

use super::{P2PSyncError, ResponseReceiver, WithPayloadSender, STEP};
use crate::client::SyncResponse;
use crate::BUFFER_SIZE;

pub type DataStreamResult = Result<Box<dyn BlockData>, P2PSyncError>;

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

pub(crate) trait DataStreamBuilder<InputFromNetwork>
where
    InputFromNetwork: Send + 'static,
    DataOrFin<InputFromNetwork>: TryFrom<Vec<u8>, Error = ProtobufConversionError>,
{
    type Output: BlockData + 'static;

    const TYPE_DESCRIPTION: &'static str;
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit;

    // Async functions in trait don't work well with argument references
    fn parse_data_for_block<'a>(
        data_receiver: &'a mut ResponseReceiver<InputFromNetwork>,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, P2PSyncError>>;

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError>;

    fn create_stream<TQuery: Send + 'static>(
        mut payload_sender: WithPayloadSender<TQuery, DataOrFin<InputFromNetwork>>,
        storage_reader: StorageReader,
        wait_period_for_new_data: Duration,
        num_blocks_per_query: u64,
        stop_sync_at_block_number: Option<BlockNumber>,
    ) -> BoxStream<'static, DataStreamResult> {
        stream! {
            let mut current_block_number = Self::get_start_block_number(&storage_reader)?;
            'send_query_and_parse_responses: loop {
                let limit = match Self::BLOCK_NUMBER_LIMIT {
                    BlockNumberLimit::Unlimited => num_blocks_per_query,
                    BlockNumberLimit::HeaderMarker => {
                        let last_block_number = storage_reader.begin_ro_txn()?.get_header_marker()?;
                        let limit = min(
                            last_block_number.0 - current_block_number.0,
                            num_blocks_per_query,
                        );
                        if limit == 0 {
                            debug!("{:?} sync is waiting for a new header", Self::TYPE_DESCRIPTION);
                            tokio::time::sleep(wait_period_for_new_data).await;
                            continue;
                        }
                        limit
                    }
                };
                let end_block_number = current_block_number.0 + limit;
                debug!(
                    "Downloading {:?} for blocks [{}, {})",
                    Self::TYPE_DESCRIPTION,
                    current_block_number.0,
                    end_block_number,
                );
                // TODO(shahak): Use the report callback.
                //TODO(Eitan): abstract report functionality to the channel struct
                let (_report_sender, report_receiver) = oneshot::channel::<()>();
                let (responses_sender, responses_receiver) = futures::channel::mpsc::channel::<SyncResponse<InputFromNetwork>>(BUFFER_SIZE);
                let responses_sender = Box::new(responses_sender);
                let mut responses_receiver: ResponseReceiver<InputFromNetwork> = Box::new(responses_receiver);
                payload_sender
                    .send(SqmrClientPayload { query:
                        Query {
                            start_block: BlockHashOrNumber::Number(current_block_number),
                            direction: Direction::Forward,
                            limit,
                            step: STEP,
                        }, report_receiver, responses_sender
                    }
                    )
                    .await?;

                while current_block_number.0 < end_block_number {
                    match Self::parse_data_for_block(
                        &mut responses_receiver, current_block_number, &storage_reader
                    ).await? {
                        Some(output) => yield Ok(Box::<dyn BlockData>::from(Box::new(output))),
                        None => {
                            debug!(
                                "Query for {:?} returned with partial data. Waiting {:?} before \
                                 sending another query.",
                                Self::TYPE_DESCRIPTION,
                                wait_period_for_new_data
                            );
                            tokio::time::sleep(wait_period_for_new_data).await;
                            continue 'send_query_and_parse_responses;
                        }
                    }
                    info!("Added {:?} for block {}.", Self::TYPE_DESCRIPTION, current_block_number);
                    current_block_number = current_block_number.unchecked_next();
                    if stop_sync_at_block_number.is_some_and(|stop_sync_at_block_number| {
                        current_block_number >= stop_sync_at_block_number
                    }) {
                        info!("{:?} hit the stop sync block number.", Self::TYPE_DESCRIPTION);
                        return;
                    }
                }

                // Consume the None message signaling the end of the query.
                match responses_receiver.next().await {
                    Some(Ok(DataOrFin(None))) => {
                        debug!("Query sent to network for {:?} finished", Self::TYPE_DESCRIPTION);
                    },
                    Some(_) => Err(P2PSyncError::TooManyResponses)?,
                    None => Err(P2PSyncError::ReceiverChannelTerminated {
                        type_description: Self::TYPE_DESCRIPTION
                    })?,
                }
            }
        }
        .boxed()
    }
}
