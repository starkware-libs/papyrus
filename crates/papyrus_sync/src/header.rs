use std::sync::Arc;
use std::time::Duration;

use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use papyrus_storage::ommer::{OmmerStorageReader, OmmerStorageWriter};
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::{StorageReader, StorageWriter};
use starknet_api::block::{BlockHeader, BlockNumber};
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tracing::{info, trace};

use crate::sources::CentralSourceTrait;
use crate::{StateSyncError, StateSyncResult};

pub async fn sync_block_while_ok<TCentralSource: CentralSourceTrait + Sync + Send>(
    reader: StorageReader,
    writer: Arc<Mutex<StorageWriter>>,
    central_source: Arc<TCentralSource>,
    block_propagation_sleep_duration: Duration,
) -> StateSyncResult {
    loop {
        let header_marker = reader.begin_ro_txn()?.get_header_marker()?;
        let last_block_number = central_source.get_block_marker().await?;
        if header_marker == last_block_number {
            tokio::time::sleep(block_propagation_sleep_duration).await;
            continue;
        }

        info!("Downloading blocks [{header_marker} - {last_block_number}).");
        let mut block_stream = central_source.stream_new_blocks(header_marker, last_block_number);

        while let Some(maybe_block) = block_stream.next().await {
            let (_, block) = maybe_block?;

            for ommer_header in get_ommer_headers(reader.clone(), &block.header)? {
                revert_block(writer.clone(), ommer_header.block_number).await?;
                store_header(writer.clone(), &ommer_header).await?;
            }

            let (in_chain, maybe_parent) = parent_in_chain(reader.clone(), &block.header)?;
            if in_chain {
                store_header(writer.clone(), &block.header).await?;
            } else if let Some(parent) = maybe_parent {
                revert_block(writer.clone(), parent.block_number).await?;
            }
        }
    }
}

async fn revert_block(
    writer: Arc<Mutex<StorageWriter>>,
    block_number: BlockNumber,
) -> StateSyncResult {
    let mut locked_writer = writer.lock().await;
    let mut txn = locked_writer.begin_rw_txn()?;

    let (updated_txn, maybe_header) = txn.revert_header(block_number)?;
    txn = updated_txn;
    if let Some(header) = maybe_header {
        info!("Reverting block header {}.", block_number);
        trace!("Block header {header:#?}");
        txn = txn.insert_ommer_header(header.block_hash, &header)?;

        let (updated_txn, maybe_body) = txn.revert_body(block_number)?;
        txn = updated_txn;
        if let Some((txs, tx_outputs, events)) = maybe_body {
            info!("Reverting block body {}.", block_number);
            trace!("Block body with transactions: {txs:#?}");
            trace!("Block body with transaction outputs: {tx_outputs:#?}");
            trace!("Block body with events: {events:#?}");
            txn = txn.insert_ommer_body(header.block_hash, &txs, &tx_outputs, &events)?;
        }

        let (updated_txn, maybe_state_diff) = txn.revert_state_diff(block_number)?;
        txn = updated_txn;
        if let Some((diff, classes)) = maybe_state_diff {
            info!("Reverting state diff {}.", block_number);
            trace!("State diff {diff:#?}");
            txn = txn.insert_ommer_state_diff(header.block_hash, &diff, &classes)?;
        }
    }

    txn.commit()?;
    Ok(())
}

async fn store_header(writer: Arc<Mutex<StorageWriter>>, header: &BlockHeader) -> StateSyncResult {
    let mut locked_writer = writer.lock().await;
    info!("Storing block header {} with hash {}.", header.block_number, header.block_hash);
    trace!("Block header data: {header:#?}");
    locked_writer.begin_rw_txn()?.append_header(header.block_number, header)?.commit()?;
    Ok(())
}

fn get_ommer_headers(
    reader: StorageReader,
    header: &BlockHeader,
) -> Result<Vec<BlockHeader>, StateSyncError> {
    if parent_in_chain(reader.clone(), header)?.0 {
        return Ok(vec![]);
    }

    let mut curr_header = header.clone();
    let mut ommer_headers = vec![];
    let txn = reader.begin_ro_txn()?;
    while let Some(ommer_prev_header) = txn.get_ommer_header(curr_header.parent_hash)? {
        if ommer_prev_header.block_number.next() != curr_header.block_number {
            break;
        }
        curr_header = ommer_prev_header.clone();
        ommer_headers.push(ommer_prev_header.clone());
        if parent_in_chain(reader.clone(), &curr_header)?.0 {
            return Ok(ommer_headers);
        }
    }

    Ok(vec![])
}

fn parent_in_chain(
    reader: StorageReader,
    header: &BlockHeader,
) -> Result<(bool, Option<BlockHeader>), StateSyncError> {
    if let Some(prev_block_number) = header.block_number.prev() {
        if let Some(prev_header) = reader.begin_ro_txn()?.get_block_header(prev_block_number)? {
            return Ok((prev_header.block_hash == header.parent_hash, Some(prev_header)));
        }

        return Ok((false, None));
    }

    Ok((true, None))
}
