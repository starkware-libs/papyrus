use std::collections::HashMap;
use std::str::FromStr;

use mockito::mock;
use web3::types::H160;

use crate::starknet::serde_utils::bytes_from_hex_str;
use crate::starknet::{
    shash, BlockHash, BlockNumber, BlockTimestamp, CallData, ClassHash, ContractAddress,
    EntryPointSelector, EthAddress, Fee, GasPrice, GlobalRoot, L1ToL2Payload, Nonce, StarkHash,
    StorageEntry, StorageKey, TransactionHash, TransactionSignature, TransactionVersion,
};
use crate::starknet_client::objects::transaction::{
    BuiltinInstanceCounter, ExecutionResources, L1ToL2Message, L1ToL2Nonce,
    TransactionIndexInBlock, TransactionReceipt,
};

// TODO(dan): use SN structs once available & sort.
use super::objects::block::{BlockStateUpdate, BlockStatus, StateDiff};
use super::objects::transaction::{
    DeclareTransaction, EntryPointType, InvokeTransaction, Transaction, TransactionType,
};
use super::{Block, ClientError, StarknetClient, StarknetError, StarknetErrorCode};

// TODO(dan): Once clash_hash is always prefixed, revert and use Core ClassHash & DeployedContract.
use super::objects::block::NonPrefixedDeployedContract;
use super::objects::NonPrefixedClassHash;

fn block_body() -> &'static str {
    r#"{
            "block_hash": "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5483",
            "parent_block_hash": "0x7d74dfc2bd87ac89447a56c51abc9b6d9aca1de21cc25fd9922f3a3779ec72d",
            "block_number": 20,
            "state_root": "07e809a2dfbe95a40ad8e5a12683e8d64c194e67a1b440027bfce23683e9fb48",
            "timestamp": 1636991716,
            "sequencer_address": "0x37b2cd6baaa515f520383bee7b7094f892f4c770695fc329a8973e841a971ae",
            "status": "ACCEPTED_ON_L1",
            "gas_price": "0x174876e800",
            "transaction_receipts": [
                {
                    "transaction_index": 33,
                    "transaction_hash": "0x01f93a441530c63350276f2c720405e8f52c8f2d695e8a4ad0f2eb0488476add",
                    "l1_to_l2_consumed_message": {
                        "from_address": "0x2Db8c2615db39a5eD8750B87aC8F217485BE11EC",
                        "to_address": "0x55a46448decca3b138edf0104b7a47d41365b8293bdfd59b03b806c102b12b7",
                        "selector": "0xc73f681176fc7b3f9693986fd7b14581e8d540519e27400e88b8713932be01",
                        "payload": [
                            "0xbc614e",
                            "0x258"
                        ]
                    },
                    "l2_to_l1_messages": [],
                    "events": [],
                    "execution_resources":
                    {
                        "n_steps": 5418,
                        "builtin_instance_counter":
                        {
                            "bitwise_builtin": 1,
                            "ec_op_builtin": 2
                        },
                        "n_memory_holes": 213
                    },
                    "actual_fee": "0x0"
                }
            ],
            "transactions": [
                {
                    "contract_address": "0x639897809c39093765f34d76776b8d081904ab30184f694f20224723ef07863",
                    "entry_point_selector": "0x15d40a3d6ca2ac30f4031e42be28da9b056fef9bb7357ac5e85627ee876e5ad",
                    "entry_point_type": "EXTERNAL",
                    "calldata": [
                        "0x3",
                        "0x4bc8ac16658025bff4a3bd0760e84fcf075417a4c55c6fae716efdd8f1ed26c"
                    ],
                    "signature": [
                        "0xbe0d6cdf1333a316ab03b7f057ee0c66716d3d983fa02ad4c46389cbe3bb75",
                        "0x396ec012117a44f204e3b501217502c9b261ef5d3da341757026df844a99d4a"
                    ],
                    "transaction_hash": "0xb7bcb42e0cfb09e38a2c21061f72d36271cc8cf13647938d4e41066c051ea8",
                    "max_fee": "0x6e0917047fd8",
                    "type": "INVOKE_FUNCTION"
                }
            ]
        }"#
}

#[tokio::test]
async fn get_block_number() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();

    // There are blocks in Starknet.
    let mock_block = mock("GET", "/feeder_gateway/get_block")
        .with_status(200)
        .with_body(block_body())
        .create();
    let block_number = starknet_client.block_number().await.unwrap();
    mock_block.assert();
    assert_eq!(block_number.unwrap(), BlockNumber(20));

    // There are no blocks in Starknet.
    let body = r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number 0 was not found."}"#;
    let mock_no_block = mock("GET", "/feeder_gateway/get_block")
        .with_status(500)
        .with_body(body)
        .create();
    let block_number = starknet_client.block_number().await.unwrap();
    mock_no_block.assert();
    assert!(block_number.is_none());
}

#[tokio::test]
async fn declare_tx_serde() {
    let declare_tx = DeclareTransaction {
        class_hash: ClassHash(shash!(
            "0x7319e2f01b0947afd86c0bb0e95029551b32f6dc192c47b2e8b08415eebbc25"
        )),
        sender_address: ContractAddress(shash!("0x1")),
        nonce: Nonce(shash!("0x0")),
        max_fee: Fee(0),
        version: TransactionVersion(shash!("0x1")),
        transaction_hash: TransactionHash(shash!(
            "0x2f2ef64daffdc72bf33b34ad024891691b8eb1d0ab70cc7f8fb71f6fd5e1f22"
        )),
        signature: TransactionSignature(vec![]),
        r#type: TransactionType::Declare,
    };
    let raw_declare_tx = serde_json::to_string(&declare_tx).unwrap();
    assert_eq!(declare_tx, serde_json::from_str(&raw_declare_tx).unwrap());
}

#[tokio::test]
async fn test_state_update() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let body = r#"
    {
        "block_hash": "0x3f65ef25e87a83d92f32f5e4869a33580f9db47ec980c1ff27bdb5151914de5",
        "new_root": "02ade8eea6eb6523d22a408a1f035bd351a9a5dce28926ca92d7abb490c0e74a",
        "old_root": "0465b219d93bcb2776aa3abb009423be3e2d04dba6453d7e027830740cd699a4",
        "state_diff":
        {
            "storage_diffs":
            {
                "0x13386f165f065115c1da38d755be261023c32f0134a03a8e66b6bb1e0016014":
                [
                    {
                        "key": "0x3b3a699bb6ef37ff4b9c4e14319c7d8e9c9bdd10ff402d1ebde18c62ae58381",
                        "value": "0x61454dd6e5c83621e41b74c"
                    },
                    {
                        "key": "0x1557182e4359a1f0c6301278e8f5b35a776ab58d39892581e357578fb287836",
                        "value": "0x79dd8085e3e5a96ea43e7d"
                    }
                ]
            },
            "deployed_contracts":
            [
                {
                    "address": "0x3e10411edafd29dfe6d427d03e35cb261b7a5efeee61bf73909ada048c029b9",
                    "class_hash": "071c3c99f5cf76fc19945d4b8b7d34c7c5528f22730d56192b50c6bbfd338a64"
                }
            ]
        }
    }"#;
    let mock = mock("GET", "/feeder_gateway/get_state_update?blockNumber=123456")
        .with_status(200)
        .with_body(body)
        .create();
    let state_update = starknet_client
        .state_update(BlockNumber(123456))
        .await
        .unwrap();
    mock.assert();
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
            storage_diffs: HashMap::from([(
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
            deployed_contracts: vec![NonPrefixedDeployedContract {
                address: ContractAddress(shash!(
                    "0x3e10411edafd29dfe6d427d03e35cb261b7a5efeee61bf73909ada048c029b9"
                )),
                class_hash: NonPrefixedClassHash(StarkHash(
                    bytes_from_hex_str::<32, false>(
                        "071c3c99f5cf76fc19945d4b8b7d34c7c5528f22730d56192b50c6bbfd338a64",
                    )
                    .unwrap(),
                )),
            }],
            declared_contracts: vec![],
        },
    };
    assert_eq!(state_update, expected_state_update);
    // }
}

#[tokio::test]
async fn get_block() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let mock = mock("GET", "/feeder_gateway/get_block?blockNumber=20")
        .with_status(200)
        .with_body(block_body())
        .create();
    let block = starknet_client.block(BlockNumber(20)).await.unwrap();
    mock.assert();
    let expected_block = Block {
        block_hash: BlockHash(shash!(
            "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5483"
        )),
        parent_block_hash: BlockHash(shash!(
            "0x7d74dfc2bd87ac89447a56c51abc9b6d9aca1de21cc25fd9922f3a3779ec72d"
        )),
        gas_price: GasPrice(0x174876e800),
        block_number: BlockNumber(20),
        sequencer_address: ContractAddress(shash!(
            "0x37b2cd6baaa515f520383bee7b7094f892f4c770695fc329a8973e841a971ae"
        )),
        status: BlockStatus::AcceptedOnL1,
        timestamp: BlockTimestamp(1636991716),
        state_root: GlobalRoot(StarkHash(
            bytes_from_hex_str::<32, false>(
                "07e809a2dfbe95a40ad8e5a12683e8d64c194e67a1b440027bfce23683e9fb48",
            )
            .unwrap(),
        )),
        transactions: vec![Transaction::Invoke(InvokeTransaction {
            calldata: CallData(vec![
                (shash!("0x3")),
                (shash!("0x4bc8ac16658025bff4a3bd0760e84fcf075417a4c55c6fae716efdd8f1ed26c")),
            ]),
            contract_address: ContractAddress(shash!(
                "0x639897809c39093765f34d76776b8d081904ab30184f694f20224723ef07863"
            )),
            entry_point_selector: EntryPointSelector(shash!(
                "0x15d40a3d6ca2ac30f4031e42be28da9b056fef9bb7357ac5e85627ee876e5ad"
            )),
            entry_point_type: EntryPointType::External,
            max_fee: Fee(0x6e0917047fd8),
            signature: TransactionSignature(vec![
                (shash!("0xbe0d6cdf1333a316ab03b7f057ee0c66716d3d983fa02ad4c46389cbe3bb75")),
                (shash!("0x396ec012117a44f204e3b501217502c9b261ef5d3da341757026df844a99d4a")),
            ]),
            transaction_hash: TransactionHash(shash!(
                "0xb7bcb42e0cfb09e38a2c21061f72d36271cc8cf13647938d4e41066c051ea8"
            )),
            r#type: TransactionType::InvokeFunction,
            version: TransactionVersion::default(),
        })],
        transaction_receipts: vec![TransactionReceipt {
            transaction_index: TransactionIndexInBlock(33),
            transaction_hash: TransactionHash(shash!(
                "0x01f93a441530c63350276f2c720405e8f52c8f2d695e8a4ad0f2eb0488476add"
            )),
            l1_to_l2_consumed_message: L1ToL2Message {
                from_address: EthAddress(
                    H160::from_str("0x2Db8c2615db39a5eD8750B87aC8F217485BE11EC").unwrap(),
                ),
                to_address: ContractAddress(shash!(
                    "0x55a46448decca3b138edf0104b7a47d41365b8293bdfd59b03b806c102b12b7"
                )),
                selector: EntryPointSelector(shash!(
                    "0xc73f681176fc7b3f9693986fd7b14581e8d540519e27400e88b8713932be01"
                )),
                payload: L1ToL2Payload(vec![shash!("0xbc614e"), shash!("0x258")]),
                nonce: L1ToL2Nonce::default(),
            },
            l2_to_l1_messages: vec![],
            events: vec![],
            execution_resources: ExecutionResources {
                n_steps: 5418,
                builtin_instance_counter: BuiltinInstanceCounter::NonEmpty(HashMap::from([
                    ("bitwise_builtin".to_string(), 1),
                    ("ec_op_builtin".to_string(), 2),
                ])),
                n_memory_holes: 213,
            },
            actual_fee: Fee(0),
        }],
    };
    assert_eq!(block, expected_block);
}

#[tokio::test]
async fn test_block_not_found_error_code() {
    let starknet_client = StarknetClient::new(&mockito::server_url()).unwrap();
    let body = r#"{"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number 2347239846 was not found."}"#;
    let mock = mock("GET", "/feeder_gateway/get_block?blockNumber=2347239846")
        .with_status(500)
        .with_body(body)
        .create();
    let err = starknet_client
        .block(BlockNumber(2347239846))
        .await
        .unwrap_err();
    mock.assert();
    assert_matches!(
        err,
        ClientError::StarknetError(StarknetError {
            code: StarknetErrorCode::BlockNotFound,
            message: msg
        }) if msg == "Block number 2347239846 was not found."
    );
}
