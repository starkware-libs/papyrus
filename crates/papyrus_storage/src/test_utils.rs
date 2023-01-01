use std::env;
use std::fs::read_to_string;
use std::path::Path;

use indexmap::IndexMap;
use starknet_api::block::{Block, BlockBody, BlockHash, BlockHeader, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress, GlobalRoot, Nonce, PatriciaKey};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{ContractClass, StateDiff, StorageKey};
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
    open_storage(config).unwrap()
}

pub fn read_json_file(path_in_resource_dir: &str) -> serde_json::Value {
    // Reads from the directory containing the manifest at run time, same as current working
    // directory.
    let path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("resources")
        .join(path_in_resource_dir);
    let json_str = read_to_string(path.to_str().unwrap()).unwrap();
    serde_json::from_str(&json_str).unwrap()
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

    BlockBody { transactions, transaction_outputs }
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

// TODO(anatg): Use impl_get_test_instance macro to implement GetTestInstance
// for StateDiff instead of this function.
pub fn get_test_state_diff() -> StateDiff {
    let address = ContractAddress::default();
    let hash = ClassHash::default();

    StateDiff {
        deployed_contracts: IndexMap::from([(address, hash)]),
        storage_diffs: IndexMap::from([(
            address,
            IndexMap::from([(StorageKey::default(), StarkFelt::default())]),
        )]),
        declared_classes: IndexMap::from([(hash, ContractClass::default())]),
        nonces: IndexMap::from([(address, Nonce::default())]),
    }
}
