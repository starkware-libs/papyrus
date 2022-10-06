use std::env;
use std::fs::read_to_string;
use std::path::Path;

use starknet_api::serde_utils::bytes_from_hex_str;
use starknet_api::{
    shash, Block, BlockBody, BlockHash, BlockHeader, BlockNumber, BlockTimestamp, CallData,
    ClassHash, ContractAddress, ContractAddressSalt, DeployTransaction, DeployTransactionOutput,
    EntryPointSelector, EthAddress, Event, EventContent, EventData, Fee, GasPrice, GlobalRoot,
    InvokeTransaction, InvokeTransactionOutput, L2ToL1Payload, MessageToL1, Nonce, StarkHash,
    Transaction, TransactionHash, TransactionOutput, TransactionSignature, TransactionVersion,
};
use tempfile::tempdir;
use web3::types::H160;

use super::{open_storage, StorageReader, StorageWriter};
use crate::db::DbConfig;

pub fn get_test_config() -> DbConfig {
    let dir = tempdir().unwrap();
    DbConfig {
        path: dir.path().to_str().unwrap().to_string(),
        max_size: 1 << 35, // 32GB.
    }
}
pub fn get_test_storage() -> (StorageReader, StorageWriter) {
    let config = get_test_config();
    open_storage(config).expect("Failed to open storage.")
}

pub fn read_resource_file(path_in_resource_dir: &str) -> Result<String, anyhow::Error> {
    let path = Path::new(&env::current_dir()?).join("resources").join(path_in_resource_dir);
    Ok(read_to_string(path.to_str().unwrap())?.replace('\n', "").replace(' ', ""))
}

pub fn get_test_body(transaction_count: usize) -> BlockBody {
    let mut transactions = vec![];
    let mut transaction_outputs = vec![];
    for i in 0..transaction_count {
        let transaction = Transaction::Deploy(DeployTransaction {
            transaction_hash: TransactionHash(StarkHash::from_u64(i as u64)),
            version: TransactionVersion(shash!("0x1")),
            contract_address: ContractAddress::try_from(shash!("0x2")).unwrap(),
            constructor_calldata: CallData(vec![shash!("0x3")]),
            class_hash: ClassHash::new(StarkHash::from_u64(i as u64)),
            contract_address_salt: ContractAddressSalt(shash!("0x4")),
        });
        transactions.push(transaction);
        let transaction_output = TransactionOutput::Deploy(DeployTransactionOutput {
            actual_fee: Fee::default(),
            messages_sent: vec![MessageToL1 {
                to_address: EthAddress::default(),
                payload: L2ToL1Payload(vec![]),
            }],
            events: vec![
                Event {
                    from_address: ContractAddress::try_from(shash!("0x4")).unwrap(),
                    content: EventContent { keys: vec![], data: EventData(vec![shash!("0x1")]) },
                },
                Event {
                    from_address: ContractAddress::try_from(shash!("0x2")).unwrap(),
                    content: EventContent { keys: vec![], data: EventData(vec![shash!("0x2")]) },
                },
                Event {
                    from_address: ContractAddress::try_from(shash!("0x3")).unwrap(),
                    content: EventContent { keys: vec![], data: EventData(vec![shash!("0x3")]) },
                },
            ],
        });
        transaction_outputs.push(transaction_output);
    }

    BlockBody::new(transactions, transaction_outputs).unwrap()
}

pub fn get_test_block(transaction_count: usize) -> Block {
    let header = BlockHeader {
        block_hash: BlockHash::new(shash!(
            "0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5483"
        )),
        block_number: BlockNumber::new(0),
        ..BlockHeader::default()
    };

    Block { header, body: get_test_body(transaction_count) }
}

pub fn get_alpha4_block_number_1() -> Block {
    let header = BlockHeader {
        block_hash: BlockHash::new(shash!(
            "0x75e00250d4343326f322e370df4c9c73c7be105ad9f532eeb97891a34d9e4a5"
        )),
        parent_hash: BlockHash::new(shash!(
            "0x7d328a71faf48c5c3857e99f20a77b18522480956d1cd5bff1ff2df3c8b427b"
        )),
        block_number: BlockNumber::new(1),
        gas_price: GasPrice::default(),
        state_root: GlobalRoot::new(shash!(
            "0x3f04ffa63e188d602796505a2ee4f6e1f294ee29a914b057af8e75b17259d9f"
        )),
        sequencer: ContractAddress::default(),
        timestamp: BlockTimestamp::new(1636989916),
    };

    let transactions = vec![
        Transaction::Deploy(DeployTransaction {
            transaction_hash: TransactionHash(shash!(
                "0x4dd12d3b82c3d0b216503c6abf63f1ccad222461582eac82057d46c327331d2"
            )),
            version: TransactionVersion::default(),
            class_hash: ClassHash::new(shash!(
                "0x10455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"
            )),
            contract_address: ContractAddress::try_from(shash!(
                "0x543e54f26ae33686f57da2ceebed98b340c3a78e9390931bd84fb711d5caabc"
            ))
            .unwrap(),
            contract_address_salt: ContractAddressSalt(shash!(
                "0x25ad1e011d139412b19ec5284fe6e95f4e53d319056c5650042eb3322cc370d"
            )),
            constructor_calldata: CallData(vec![
                shash!("0x70be09c520814c13480a220ad31eb94bf37f0259e002b0275e55f3c309ee823"),
                shash!("0x1dc19dce5326f42f2b319d78b237148d1e582efbf700efd6eb2c9fcbc451327"),
            ]),
        }),
        Transaction::Deploy(DeployTransaction {
            transaction_hash: TransactionHash(shash!(
                "0x1a5f7247cc207f5b5c2e48b7605e46b872b83a2fa842955aea42d3cd80dbff"
            )),
            version: TransactionVersion::default(),
            class_hash: ClassHash::new(shash!(
                "0x10455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"
            )),
            contract_address: ContractAddress::try_from(shash!(
                "0x2fb7ff5b1b474e8e691f5bebad9aa7aa3009f6ef22ccc2816f96cdfe217604d"
            ))
            .unwrap(),
            contract_address_salt: ContractAddressSalt(shash!(
                "0x3a27aed698130e1817544c060261e8aede51a02f4da510c67ff26c5fbae850e"
            )),
            constructor_calldata: CallData(vec![
                shash!("0x420eefdc029d53134b57551d676c9a450e5f75f9f017ca75f6fb28350f60d54"),
                shash!("0x7c7139d51f4642ec66088959e69eb890e2e6e87c08dad2a223da9161c99c939"),
            ]),
        }),
        Transaction::Deploy(DeployTransaction {
            transaction_hash: TransactionHash(shash!(
                "0x5ea9bca61575eeb4ed38a16cefcbf66ba1ed642642df1a1c07b44316791b378"
            )),
            version: TransactionVersion::default(),
            class_hash: ClassHash::new(shash!(
                "0x7b40f8e4afe1316fce16375ac7a06d4dd27c7a4e3bcd6c28afdd208c5db433d"
            )),
            contract_address: ContractAddress::try_from(shash!(
                "0x1bb929cc5e6d80f0c71e90365ab77e9cbb2e0a290d72255a3f4d34060b5ed52"
            ))
            .unwrap(),
            contract_address_salt: ContractAddressSalt::default(),
            constructor_calldata: CallData(vec![]),
        }),
        Transaction::Invoke(InvokeTransaction {
            transaction_hash: TransactionHash(shash!(
                "0x6525d9aa309e5c80abbdafcc434d53202e06866597cd6dbbc91e5894fad7155"
            )),
            max_fee: Fee::default(),
            version: TransactionVersion::default(),
            signature: TransactionSignature::default(),
            nonce: Nonce::default(),
            contract_address: ContractAddress::try_from(shash!(
                "0x2fb7ff5b1b474e8e691f5bebad9aa7aa3009f6ef22ccc2816f96cdfe217604d"
            ))
            .unwrap(),
            entry_point_selector: Some(EntryPointSelector(shash!(
                "0x12ead94ae9d3f9d2bdb6b847cf255f1f398193a1f88884a0ae8e18f24a037b6"
            ))),
            calldata: CallData(vec![shash!("0xe3402af6cc1bca3f22d738ab935a5dd8ad1fb230")]),
        }),
    ];

    let transaction_outputs = vec![
        TransactionOutput::Deploy(DeployTransactionOutput {
            actual_fee: Fee::default(),
            messages_sent: vec![],
            events: vec![],
        }),
        TransactionOutput::Deploy(DeployTransactionOutput {
            actual_fee: Fee::default(),
            messages_sent: vec![],
            events: vec![],
        }),
        TransactionOutput::Deploy(DeployTransactionOutput {
            actual_fee: Fee::default(),
            messages_sent: vec![],
            events: vec![],
        }),
        TransactionOutput::Invoke(InvokeTransactionOutput {
            actual_fee: Fee::default(),
            messages_sent: vec![MessageToL1 {
                to_address: EthAddress(H160(
                    bytes_from_hex_str::<20, true>("0xe3402aF6cc1BCa3f22D738AB935a5Dd8AD1Fb230")
                        .unwrap(),
                )),
                payload: L2ToL1Payload(vec![shash!("0xc"), shash!("0x22")]),
            }],
            events: vec![],
        }),
    ];

    Block { header, body: BlockBody::new(transactions, transaction_outputs).unwrap() }
}
