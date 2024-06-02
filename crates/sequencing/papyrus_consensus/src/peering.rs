use std::convert::TryFrom;

use async_channel as mpmc;
use futures::channel::{mpsc, oneshot};
use futures::{SinkExt, StreamExt};
use papyrus_network::network_manager::{BroadcastSubscriberChannels, ReportCallback};
use papyrus_network::sqmr::Bytes;
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal};
use papyrus_protobuf::converters::ProtobufConversionError;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::Transaction;

use crate::types::{PeeringConsensusMessage, ProposalInit};

type ShcMessage = PeeringConsensusMessage<Transaction>;

// TODO(matan): Support streaming.
pub struct Peering {
    from_shc_receiver: mpsc::Receiver<ShcMessage>,
    to_shc_sender: mpmc::Sender<ShcMessage>,
    network_channels:
        BroadcastSubscriberChannels<ConsensusMessage, <ConsensusMessage as TryFrom<Bytes>>::Error>,
    content_buffer_size: usize,
}

impl Peering {
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                msg = self.from_shc_receiver.next() => {
                    match msg {
                        Some(m) => self.msg_from_shc(m).await,
                        None => break,
                    }
                }
                msg = self.network_channels.broadcasted_messages_receiver.next() => {
                    match msg {
                        Some((m, callback)) => self.msg_from_network(m, callback).await,
                        None => break,
                    }
                }
            }
        }
    }

    pub(super) async fn msg_from_shc(&mut self, msg: ShcMessage) {
        match msg {
            ShcMessage::Proposal((init, mut content_receiver, fin_receiver)) => {
                // Broadcast a proposal to peers.
                let mut transactions = Vec::new();
                while let Some(txn) = content_receiver.next().await {
                    // Sending no content is allowed.
                    transactions.push(txn)
                }
                let Ok(block_hash) = fin_receiver.await else {
                    // SHC dropped sender before completing the proposal. While unexpected this only
                    // impacts the current proposal.
                    //
                    // TODO(matan): Log error.
                    return;
                };
                let proposal = Proposal {
                    height: init.height.0,
                    proposer: init.proposer.into(),
                    transactions,
                    block_hash,
                };
                self.network_channels
                    .messages_to_broadcast_sender
                    .send(ConsensusMessage::Proposal(proposal))
                    .await
                    .unwrap();
            }
        }
    }

    pub(super) async fn msg_from_network(
        &mut self,
        msg: Result<ConsensusMessage, ProtobufConversionError>,
        report_callback: ReportCallback,
    ) {
        match msg {
            Ok(m) => self.msg_from_network_impl(m, report_callback).await,
            Err(_) => {
                // TODO(matan): Log error.
                report_callback();
            }
        }
    }

    // The actual logic for handling messages from other nodes.
    //
    // In the future, when we add validations (e.g. signatures), `report_callback` will be used.
    pub(super) async fn msg_from_network_impl(
        &mut self,
        msg: ConsensusMessage,
        _report_callback: ReportCallback,
    ) {
        match msg {
            ConsensusMessage::Proposal(proposal) => {
                let init = ProposalInit {
                    height: BlockNumber(proposal.height),
                    proposer: proposal.proposer,
                };
                let (mut content_sender, content_receiver) =
                    mpsc::channel(self.content_buffer_size);
                let (fin_sender, fin_receiver) = oneshot::channel();
                self.to_shc_sender
                    .send(ShcMessage::Proposal((init, content_receiver, fin_receiver)))
                    .await
                    .expect("Cannot send to SingleHeightConsensus");
                for txn in proposal.transactions {
                    if content_sender.send(txn).await.is_err() {
                        // SHC dropped receiver before completing the proposal. While unexpected
                        // this only impacts the current proposal.
                        //
                        // TODO(matan): Log error.
                        return;
                    }
                }
                if fin_sender.send(proposal.block_hash).is_err() {
                    // SHC dropped receiver before completing the proposal. While unexpected this
                    // only impacts the current proposal.
                    //
                    // TODO(matan): Log error.
                }
            }
        }
    }
}
