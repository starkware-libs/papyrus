use std::env;
use std::fs::read_to_string;
use std::path::Path;

use starknet_api::block::{Block, BlockBody, BlockHash, BlockHeader, BlockNumber, GlobalRoot};
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::hash::StarkHash;
use starknet_api::state::{
    ContractClass, ContractNonce, DeclaredContract, DeployedContract, StateDiff, StorageDiff,
    StorageEntry, StorageKey,
};
use starknet_api::transaction::{
    CallData, ContractAddressSalt, DeployTransaction, DeployTransactionOutput, EthAddress, Event,
    EventContent, EventData, EventKey, Fee, L2ToL1Payload, MessageToL1, Transaction,
    TransactionHash, TransactionOutput, TransactionVersion,
};
use starknet_api::{patky, shash};
use tempfile::tempdir;

use crate::db::DbConfig;
use crate::{open_storage, StorageReader, StorageWriter};

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

pub fn read_json_file(path_in_resource_dir: &str) -> Result<serde_json::Value, anyhow::Error> {
    // Reads from the directory containing the manifest at run time, same as current working
    // directory.
    read_json_file_from_dir(&env::var("CARGO_MANIFEST_DIR")?, path_in_resource_dir)
}

fn read_json_file_from_storage_resources(
    path_in_resource_dir: &str,
) -> Result<serde_json::Value, anyhow::Error> {
    // Reads from the directory containing the manifest at compile time, which is the storage crate
    // directory.
    read_json_file_from_dir(env!("CARGO_MANIFEST_DIR"), path_in_resource_dir)
}

fn read_json_file_from_dir(
    dir: &str,
    path_in_resource_dir: &str,
) -> Result<serde_json::Value, anyhow::Error> {
    let path = Path::new(dir).join("resources").join(path_in_resource_dir);
    let json_str = read_to_string(path.to_str().unwrap())?;
    Ok(serde_json::from_str(&json_str)?)
}

/// Returns a test block body with a variable number of transactions.
pub fn get_test_body(transaction_count: usize) -> BlockBody {
    let mut transactions = vec![];
    let mut transaction_outputs = vec![];
    for i in 0..transaction_count {
        let transaction = Transaction::Deploy(DeployTransaction {
            transaction_hash: TransactionHash(StarkHash::from(i as u64)),
            version: TransactionVersion(shash!("0x1")),
            contract_address: ContractAddress(patky!("0x2")),
            constructor_calldata: CallData(vec![shash!("0x3")]),
            class_hash: ClassHash(StarkHash::from(i as u64)),
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
                    from_address: ContractAddress(patky!("0x22")),
                    content: EventContent {
                        keys: vec![EventKey(shash!("0x7")), EventKey(shash!("0x6"))],
                        data: EventData(vec![shash!("0x1")]),
                    },
                },
                Event {
                    from_address: ContractAddress(patky!("0x22")),
                    content: EventContent {
                        keys: vec![EventKey(shash!("0x6"))],
                        data: EventData(vec![shash!("0x2")]),
                    },
                },
                Event {
                    from_address: ContractAddress(patky!("0x23")),
                    content: EventContent {
                        keys: vec![EventKey(shash!("0x7"))],
                        data: EventData(vec![shash!("0x3")]),
                    },
                },
                Event {
                    from_address: ContractAddress(patky!("0x22")),
                    content: EventContent {
                        keys: vec![EventKey(shash!("0x9"))],
                        data: EventData(vec![shash!("0x4")]),
                    },
                },
                Event {
                    from_address: ContractAddress(patky!("0x22")),
                    content: EventContent {
                        keys: vec![EventKey(shash!("0x6")), EventKey(shash!("0x7"))],
                        data: EventData(vec![shash!("0x5")]),
                    },
                },
            ],
        });
        transaction_outputs.push(transaction_output);
    }

    BlockBody::new(transactions, transaction_outputs).unwrap()
}

pub fn get_test_block(transaction_count: usize) -> Block {
    let header = BlockHeader {
        block_hash: BlockHash(shash!(
            "0x7d328a71faf48c5c3857e99f20a77b18522480956d1cd5bff1ff2df3c8b427b"
        )),
        block_number: BlockNumber(0),
        state_root: GlobalRoot(shash!(
            "0x02c2bb91714f8448ed814bdac274ab6fcdbafc22d835f9e847e5bee8c2e5444e"
        )),
        ..BlockHeader::default()
    };

    Block { header, body: get_test_body(transaction_count) }
}

pub fn get_test_state_diff() -> (BlockHeader, BlockHeader, StateDiff, Vec<DeclaredContract>) {
    let parent_hash =
        BlockHash(shash!("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5483"));
    let state_root = GlobalRoot(shash!("0x12"));
    let parent_header = BlockHeader {
        block_number: BlockNumber(0),
        block_hash: parent_hash,
        state_root,
        ..BlockHeader::default()
    };

    let block_hash =
        BlockHash(shash!("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5493"));
    let header = BlockHeader {
        block_number: BlockNumber(1),
        block_hash,
        parent_hash,
        ..BlockHeader::default()
    };

    let address0 = ContractAddress(patky!(
        "0x543e54f26ae33686f57da2ceebed98b340c3a78e9390931bd84fb711d5caabc"
    ));
    let hash0 =
        ClassHash(shash!("0x10455c752b86932ce552f2b0fe81a880746649b9aee7e0d842bf3f52378f9f8"));
    let class_value = read_json_file_from_storage_resources("contract_class.json").unwrap();
    let class0 = serde_json::from_value(class_value).unwrap();
    let address1 = ContractAddress(patky!("0x21"));
    let hash1 = ClassHash(shash!("0x5"));
    let class1 = ContractClass::default();
    let hash2 = ClassHash(shash!("0x6"));
    let class2 = ContractClass::default();

    let key0 =
        StorageKey(patky!("0x70be09c520814c13480a220ad31eb94bf37f0259e002b0275e55f3c309ee823"));
    let value0 = shash!("0x1dc19dce5326f42f2b319d78b237148d1e582efbf700efd6eb2c9fcbc451327");
    let key1 =
        StorageKey(patky!("0x420eefdc029d53134b57551d676c9a450e5f75f9f017ca75f6fb28350f60d54"));
    let value1 = shash!("0x7c7139d51f4642ec66088959e69eb890e2e6e87c08dad2a223da9161c99c939");

    let diff = StateDiff::new(
        vec![
            DeployedContract { address: address0, class_hash: hash0 },
            DeployedContract { address: address1, class_hash: hash1 },
        ],
        vec![
            StorageDiff::new(
                address0,
                vec![
                    StorageEntry { key: key0, value: value0 },
                    StorageEntry { key: key1, value: value1 },
                ],
            )
            .unwrap(),
        ],
        vec![
            DeclaredContract { class_hash: hash1, contract_class: class1.clone() },
            DeclaredContract { class_hash: hash2, contract_class: class2 },
        ],
        vec![ContractNonce { contract_address: address0, nonce: Nonce(StarkHash::from(1)) }],
    )
    .unwrap();

    let deployed_contract_class_definitions = vec![
        DeclaredContract { class_hash: hash0, contract_class: class0 },
        DeclaredContract { class_hash: hash1, contract_class: class1 },
    ];

    (parent_header, header, diff, deployed_contract_class_definitions)
}
