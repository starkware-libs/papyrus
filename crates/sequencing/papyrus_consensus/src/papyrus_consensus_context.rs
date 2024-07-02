#[cfg(test)]
#[path = "papyrus_consensus_context_test.rs"]
mod papyrus_consensus_context_test;

use core::panic;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::sink::SinkExt;
use futures::StreamExt;
use papyrus_network::network_manager::SubscriberSender;
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal};
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageError, StorageReader};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::transaction::Transaction;
use tokio::sync::Mutex;
use tracing::debug;

use crate::types::{ConsensusBlock, ConsensusContext, ConsensusError, ProposalInit, ValidatorId};
use crate::ProposalWrapper;

// TODO: add debug messages and span to the tasks.

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PapyrusConsensusBlock {
    content: Vec<Transaction>,
    id: BlockHash,
}

impl ConsensusBlock for PapyrusConsensusBlock {
    type ProposalChunk = Transaction;
    type ProposalIter = std::vec::IntoIter<Transaction>;

    fn id(&self) -> BlockHash {
        self.id
    }

    fn proposal_iter(&self) -> Self::ProposalIter {
        self.content.clone().into_iter()
    }
}

pub struct PapyrusConsensusContext {
    storage_reader: StorageReader,
    broadcast_sender: Arc<Mutex<SubscriberSender<ConsensusMessage>>>,
}

impl PapyrusConsensusContext {
    // TODO(dvir): remove the dead code attribute after we will use this function.
    #[allow(dead_code)]
    pub fn new(
        storage_reader: StorageReader,
        broadcast_sender: SubscriberSender<ConsensusMessage>,
    ) -> Self {
        Self { storage_reader, broadcast_sender: Arc::new(Mutex::new(broadcast_sender)) }
    }
}

const CHANNEL_SIZE: usize = 5000;

#[async_trait]
impl ConsensusContext for PapyrusConsensusContext {
    type Block = PapyrusConsensusBlock;

    async fn build_proposal(
        &self,
        height: BlockNumber,
    ) -> (mpsc::Receiver<Transaction>, oneshot::Receiver<PapyrusConsensusBlock>) {
        let (mut sender, receiver) = mpsc::channel(CHANNEL_SIZE);
        let (fin_sender, fin_receiver) = oneshot::channel();

        let storage_reader = self.storage_reader.clone();
        tokio::spawn(async move {
            // TODO(dvir): consider fix this for the case of reverts. If between the check that the
            // block in storage and to getting the transaction was a revert this flow will fail.
            wait_for_block(&storage_reader, height).await.expect("Failed to wait to block");

            let txn = storage_reader.begin_ro_txn().expect("Failed to begin ro txn");
            let transactions = txn
                .get_block_transactions(height)
                .expect("Get transactions from storage failed")
                .unwrap_or_else(|| {
                    panic!("Block in {height} was not found in storage despite waiting for it")
                });

            for tx in transactions.clone() {
                sender.try_send(tx).expect("Send should succeed");
            }
            sender.close_channel();

            let block_hash = txn
                .get_block_header(height)
                .expect("Get header from storage failed")
                .unwrap_or_else(|| {
                    panic!("Block in {height} was not found in storage despite waiting for it")
                })
                .block_hash;
            fin_sender
                .send(PapyrusConsensusBlock { content: transactions, id: block_hash })
                .expect("Send should succeed");
        });

        (receiver, fin_receiver)
    }

    async fn validate_proposal(
        &self,
        height: BlockNumber,
        mut content: mpsc::Receiver<Transaction>,
    ) -> oneshot::Receiver<PapyrusConsensusBlock> {
        let (fin_sender, fin_receiver) = oneshot::channel();

        let storage_reader = self.storage_reader.clone();
        tokio::spawn(async move {
            // TODO(dvir): consider fix this for the case of reverts. If between the check that the
            // block in storage and to getting the transaction was a revert this flow will fail.
            wait_for_block(&storage_reader, height).await.expect("Failed to wait to block");

            let txn = storage_reader.begin_ro_txn().expect("Failed to begin ro txn");
            let transactions = txn
                .get_block_transactions(height)
                .expect("Get transactions from storage failed")
                .unwrap_or_else(|| {
                    panic!("Block in {height} was not found in storage despite waiting for it")
                });

            for tx in transactions.iter() {
                let received_tx = content
                    .next()
                    .await
                    .unwrap_or_else(|| panic!("Not received transaction equals to {tx:?}"));
                if tx != &received_tx {
                    panic!("Transactions are not equal. In storage: {tx:?}, : {received_tx:?}");
                }
            }

            if content.next().await.is_some() {
                panic!("Received more transactions than expected");
            }

            let block_hash = txn
                .get_block_header(height)
                .expect("Get header from storage failed")
                .unwrap_or_else(|| {
                    panic!("Block in {height} was not found in storage despite waiting for it")
                })
                .block_hash;
            fin_sender
                .send(PapyrusConsensusBlock { content: transactions, id: block_hash })
                .expect("Send should succeed");
        });

        fin_receiver
    }

    async fn validators(&self, _height: BlockNumber) -> Vec<ValidatorId> {
        vec![0u8.into(), 1u8.into(), 2u8.into(), 3u8.into()]
    }

    fn proposer(&self, _validators: &[ValidatorId], height: BlockNumber) -> ValidatorId {
        (height.0 % 2).into()
    }

    async fn broadcast(&self, message: ConsensusMessage) -> Result<(), ConsensusError> {
        debug!("Broadcasting message: {message:?}");
        self.broadcast_sender.lock().await.send(message).await?;
        Ok(())
    }

    async fn propose(
        &self,
        init: ProposalInit,
        mut content_receiver: mpsc::Receiver<Transaction>,
        fin_receiver: oneshot::Receiver<BlockHash>,
    ) -> Result<(), ConsensusError> {
        let broadcast_sender = self.broadcast_sender.clone();

        tokio::spawn(async move {
            let mut transactions = Vec::new();
            while let Some(tx) = content_receiver.next().await {
                transactions.push(tx);
            }

            let block_hash =
                fin_receiver.await.expect("Failed to get block hash from fin receiver");
            let proposal = Proposal {
                height: init.height.0,
                proposer: init.proposer,
                transactions,
                block_hash,
            };
            debug!(
                "Sending proposal: height={:?} id={:?} num_txs={} block_hash={:?}",
                proposal.height,
                proposal.proposer,
                proposal.transactions.len(),
                proposal.block_hash
            );

            broadcast_sender
                .lock()
                .await
                .send(ConsensusMessage::Proposal(proposal))
                .await
                .expect("Failed to send proposal");
        });
        Ok(())
    }
}

const SLEEP_BETWEEN_CHECK_FOR_BLOCK: Duration = Duration::from_secs(10);

async fn wait_for_block(
    storage_reader: &StorageReader,
    height: BlockNumber,
) -> Result<(), StorageError> {
    while storage_reader.begin_ro_txn()?.get_body_marker()? <= height {
        debug!("Waiting for block {height:?} to continue consensus");
        tokio::time::sleep(SLEEP_BETWEEN_CHECK_FOR_BLOCK).await;
    }
    Ok(())
}

impl From<ProposalWrapper>
    for (ProposalInit, mpsc::Receiver<Transaction>, oneshot::Receiver<BlockHash>)
{
    fn from(val: ProposalWrapper) -> Self {
        let transactions: Vec<Transaction> = val.0.transactions.into_iter().collect();
        let proposal_init =
            ProposalInit { height: BlockNumber(val.0.height), proposer: val.0.proposer };
        let (mut content_sender, content_receiver) = mpsc::channel(transactions.len());
        for tx in transactions {
            content_sender.try_send(tx).expect("Send should succeed");
        }
        content_sender.close_channel();

        let (fin_sender, fin_receiver) = oneshot::channel();
        fin_sender.send(val.0.block_hash).expect("Send should succeed");

        (proposal_init, content_receiver, fin_receiver)
    }
}
