use futures::channel::{mpsc, oneshot};
use futures::StreamExt;
use papyrus_network::network_manager::{mock_register_broadcast_subscriber, BroadcastNetworkMock};
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal};
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use starknet_api::block::Block;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::Transaction;
use test_utils::get_test_block;

use crate::papyrus_consensus_context::PapyrusConsensusContext;
use crate::types::{ConsensusBlock, ConsensusContext, ProposalInit};

// TODO(dvir): consider adding tests for times, i.e, the calls are returned immediately and nothing
// happen until it should (for example, not creating a block before we have it in storage).

const TEST_CHANNEL_SIZE: usize = 10;

#[tokio::test]
async fn build_proposal() {
    let (block, papyrus_context, _mock_network) = test_setup();
    let block_number = block.header.block_number;

    let (mut proposal_receiver, fin_receiver) = papyrus_context.build_proposal(block_number).await;

    let mut transactions = Vec::new();
    while let Some(tx) = proposal_receiver.next().await {
        transactions.push(tx);
    }
    assert_eq!(transactions, block.body.transactions);

    let fin = fin_receiver.await.unwrap();
    assert_eq!(fin.id(), block.header.block_hash);
    assert_eq!(fin.proposal_iter().collect::<Vec::<Transaction>>(), block.body.transactions);
}

#[tokio::test]
async fn validate_proposal_success() {
    let (block, papyrus_context, _mock_network) = test_setup();
    let block_number = block.header.block_number;

    let (mut validate_sender, validate_receiver) = mpsc::channel(TEST_CHANNEL_SIZE);
    for tx in block.body.transactions.clone() {
        validate_sender.try_send(tx).unwrap();
    }
    validate_sender.close_channel();

    let fin =
        papyrus_context.validate_proposal(block_number, validate_receiver).await.await.unwrap();

    assert_eq!(fin.id(), block.header.block_hash);
    assert_eq!(fin.proposal_iter().collect::<Vec::<Transaction>>(), block.body.transactions);
}

#[tokio::test]
async fn validate_proposal_fail() {
    let (block, papyrus_context, _mock_network) = test_setup();
    let block_number = block.header.block_number;

    let different_block = get_test_block(4, None, None, None);
    let (mut validate_sender, validate_receiver) = mpsc::channel(5000);
    for tx in different_block.body.transactions.clone() {
        validate_sender.try_send(tx).unwrap();
    }
    validate_sender.close_channel();

    let fin = papyrus_context.validate_proposal(block_number, validate_receiver).await.await;
    assert_eq!(fin, Err(oneshot::Canceled));
}

#[tokio::test]
async fn propose() {
    let (block, papyrus_context, mut mock_network) = test_setup();
    let block_number = block.header.block_number;

    let (mut content_sender, content_receiver) = mpsc::channel(TEST_CHANNEL_SIZE);
    for tx in block.body.transactions.clone() {
        content_sender.try_send(tx).unwrap();
    }
    content_sender.close_channel();

    let (fin_sender, fin_receiver) = oneshot::channel();
    fin_sender.send(block.header.block_hash).unwrap();

    let proposal_init = ProposalInit { height: block_number, proposer: ContractAddress::default() };
    papyrus_context.propose(proposal_init.clone(), content_receiver, fin_receiver).await.unwrap();

    let expected_message = ConsensusMessage::Proposal(Proposal {
        height: proposal_init.height.0,
        proposer: proposal_init.proposer,
        transactions: block.body.transactions,
        block_hash: block.header.block_hash,
    });

    assert_eq!(mock_network.messages_to_broadcast_receiver.next().await.unwrap(), expected_message);
}

fn test_setup() -> (Block, PapyrusConsensusContext, BroadcastNetworkMock<ConsensusMessage>) {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let block = get_test_block(5, None, None, None);
    let block_number = block.header.block_number;
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let test_channels = mock_register_broadcast_subscriber().unwrap();
    let papyrus_context = PapyrusConsensusContext::new(
        storage_reader.clone(),
        test_channels.subscriber_channels.messages_to_broadcast_sender,
        4,
    );
    (block, papyrus_context, test_channels.mock_network)
}
