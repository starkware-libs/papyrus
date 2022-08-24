use std::collections::BTreeMap;

use assert::assert_ok;
use assert_matches::assert_matches;
use starknet_api::serde_utils::bytes_from_hex_str;
use starknet_api::{
    shash, BlockHash, ClassHash, ContractAddress, DeployedContract, GlobalRoot, StarkHash,
    StorageEntry, StorageKey, TransactionHash, TransactionOffsetInBlock,
};

use super::block::{Block, BlockStateUpdate, StateDiff, TransactionReceiptsError};
use super::transaction::TransactionReceipt;
use crate::test_utils::read_resource::read_resource_file;
use crate::ClientError;

#[test]
fn load_block_succeeds() {
    assert_ok!(serde_json::from_str::<Block>(&read_resource_file("block.json")));
}

#[test]
fn load_block_state_update_succeeds() {
    let expected_state_update = BlockStateUpdate {
        block_hash: BlockHash(shash!(
            "0x3f65ef25e87a83d92f32f5e4869a33580f9db47ec980c1ff27bdb5151914de5"
        )),
        new_root: GlobalRoot(StarkHash(
            bytes_from_hex_str::<32, false>(
                "02ade8eea6eb6523d22a408a1f035bd351a9a5dce28926ca92d7abb490c0e74a",
            )
            .unwrap(),
        )),
        old_root: GlobalRoot(StarkHash(
            bytes_from_hex_str::<32, false>(
                "0465b219d93bcb2776aa3abb009423be3e2d04dba6453d7e027830740cd699a4",
            )
            .unwrap(),
        )),
        state_diff: StateDiff {
            storage_diffs: BTreeMap::from([(
                ContractAddress(shash!(
                    "0x13386f165f065115c1da38d755be261023c32f0134a03a8e66b6bb1e0016014"
                )),
                vec![
                    StorageEntry {
                        key: StorageKey(shash!(
                            "0x3b3a699bb6ef37ff4b9c4e14319c7d8e9c9bdd10ff402d1ebde18c62ae58381"
                        )),
                        value: shash!("0x61454dd6e5c83621e41b74c"),
                    },
                    StorageEntry {
                        key: StorageKey(shash!(
                            "0x1557182e4359a1f0c6301278e8f5b35a776ab58d39892581e357578fb287836"
                        )),
                        value: shash!("0x79dd8085e3e5a96ea43e7d"),
                    },
                ],
            )]),
            deployed_contracts: vec![DeployedContract {
                address: ContractAddress(shash!(
                    "0x3e10411edafd29dfe6d427d03e35cb261b7a5efeee61bf73909ada048c029b9"
                )),
                class_hash: ClassHash(shash!(
                    "0x071c3c99f5cf76fc19945d4b8b7d34c7c5528f22730d56192b50c6bbfd338a64"
                )),
            }],
            declared_classes: vec![],
        },
    };
    assert_eq!(
        expected_state_update,
        serde_json::from_str::<BlockStateUpdate>(&read_resource_file("block_state_update.json"))
            .unwrap()
    )
}

#[tokio::test]
async fn try_into_starknet_api() {
    let raw_block = read_resource_file("block.json");
    let block: Block = serde_json::from_str(&raw_block).unwrap();
    let expected_num_of_tx_outputs = block.transactions.len();
    let starknet_api_block = starknet_api::Block::try_from(block).unwrap();
    assert_eq!(expected_num_of_tx_outputs, starknet_api_block.body.transaction_outputs.len());

    let mut err_block: Block = serde_json::from_str(&raw_block).unwrap();
    err_block.transaction_receipts.pop();
    let err = starknet_api::Block::try_from(err_block).unwrap_err();
    assert_matches!(
        err,
        ClientError::TransactionReceiptsError(TransactionReceiptsError::WrongNumberOfReceipts {
            block_number: _,
            num_of_txs: _,
            num_of_receipts: _,
        })
    );

    let mut err_block: Block = serde_json::from_str(&raw_block).unwrap();
    err_block.transaction_receipts[0].transaction_index = TransactionOffsetInBlock(1);
    let err = starknet_api::Block::try_from(err_block).unwrap_err();
    assert_matches!(
        err,
        ClientError::TransactionReceiptsError(TransactionReceiptsError::MismatchTransactionIndex {
            block_number: _,
            tx_index: _,
            tx_hash: _,
            receipt_tx_index: _,
        })
    );

    let mut err_block: Block = serde_json::from_str(&raw_block).unwrap();
    err_block.transaction_receipts[0].transaction_hash = TransactionHash(shash!("0x4"));
    let err = starknet_api::Block::try_from(err_block).unwrap_err();
    assert_matches!(
        err,
        ClientError::TransactionReceiptsError(TransactionReceiptsError::MismatchTransactionHash {
            block_number: _,
            tx_index: _,
            tx_hash: _,
            receipt_tx_hash: _,
        })
    );

    let mut err_block: Block = serde_json::from_str(&raw_block).unwrap();
    err_block.transaction_receipts[0] = TransactionReceipt {
        transaction_index: TransactionOffsetInBlock(0),
        transaction_hash: err_block.transactions[0].transaction_hash(),
        ..err_block.transaction_receipts[1].clone()
    };
    let err = starknet_api::Block::try_from(err_block).unwrap_err();
    assert_matches!(
        err,
        ClientError::TransactionReceiptsError(TransactionReceiptsError::MismatchFields {
            block_number: _,
            tx_index: _,
            tx_hash: _,
            tx_type: _,
        })
    );
}
