use std::env;
use std::fs::read_to_string;
use std::ops::Index;
use std::path::Path;

use indexmap::IndexMap;
use rand::Rng;
use starknet_api::block::{Block, BlockBody, BlockHeader};
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{ContractClass, StateDiff, StorageKey};
use starknet_api::transaction::{
    DeclareTransaction, DeclareTransactionOutput, DeployAccountTransaction,
    DeployAccountTransactionOutput, DeployTransaction, DeployTransactionOutput, Event,
    EventContent, EventData, EventKey, InvokeTransaction, InvokeTransactionOutput,
    L1HandlerTransaction, L1HandlerTransactionOutput, Transaction, TransactionHash,
    TransactionOutput,
};
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

// Returns a test block with a variable number of transactions and events.
pub fn get_test_block_with_events(
    transaction_count: usize,
    events_per_tx: usize,
    from_addresses: Option<Vec<ContractAddress>>,
    keys: Option<Vec<Vec<EventKey>>>,
) -> Block {
    Block {
        header: BlockHeader::default(),
        body: get_test_body_with_events(transaction_count, events_per_tx, from_addresses, keys),
    }
}

// Returns a test block body with a variable number of transactions and events.
pub fn get_test_body_with_events(
    transaction_count: usize,
    events_per_tx: usize,
    from_addresses: Option<Vec<ContractAddress>>,
    keys: Option<Vec<Vec<EventKey>>>,
) -> BlockBody {
    let mut body = get_test_body(transaction_count);
    let mut rng = rand::thread_rng();
    for tx_output in &mut body.transaction_outputs {
        let mut events = vec![];
        for _ in 0..events_per_tx {
            let from_address = if let Some(ref options) = from_addresses {
                *options.index(rng.gen_range(0..options.len()))
            } else {
                ContractAddress::default()
            };
            let final_keys = if let Some(ref options) = keys {
                let mut chosen_keys = vec![];
                for options_per_i in options {
                    let key = options_per_i.index(rng.gen_range(0..options_per_i.len())).clone();
                    chosen_keys.push(key);
                }
                chosen_keys
            } else {
                vec![EventKey::default()]
            };
            events.push(Event {
                from_address,
                content: EventContent { keys: final_keys, data: EventData::default() },
            });
        }
        set_events(tx_output, events);
    }
    body
}

// Returns a test block with a variable number of transactions.
pub fn get_test_block(transaction_count: usize) -> Block {
    Block { header: BlockHeader::default(), body: get_test_body(transaction_count) }
}

// Returns a test block body with a variable number of transactions.
pub fn get_test_body(transaction_count: usize) -> BlockBody {
    let mut transactions = vec![];
    let mut transaction_outputs = vec![];
    for i in 0..transaction_count {
        let mut transaction = get_test_transaction();
        set_transaction_hash(&mut transaction, TransactionHash(StarkHash::from(i as u64)));
        let transaction_output = get_test_transaction_output(&transaction);
        transactions.push(transaction);
        transaction_outputs.push(transaction_output);
    }

    BlockBody { transactions, transaction_outputs }
}

// TODO(anatg): Use get_test_instance for Transaction instead of this function.
fn get_test_transaction() -> Transaction {
    let mut rng = rand::thread_rng();
    let variant = rng.gen_range(0..5);
    match variant {
        0 => Transaction::Declare(DeclareTransaction::default()),
        1 => Transaction::Deploy(DeployTransaction::default()),
        2 => Transaction::DeployAccount(DeployAccountTransaction::default()),
        3 => Transaction::Invoke(InvokeTransaction::default()),
        4 => Transaction::L1Handler(L1HandlerTransaction::default()),
        _ => {
            panic!("Variant {:?} should match one of the enum Transaction variants.", variant);
        }
    }
}

fn get_test_transaction_output(transaction: &Transaction) -> TransactionOutput {
    match transaction {
        Transaction::Declare(_) => TransactionOutput::Declare(DeclareTransactionOutput::default()),
        Transaction::Deploy(_) => TransactionOutput::Deploy(DeployTransactionOutput::default()),
        Transaction::DeployAccount(_) => {
            TransactionOutput::DeployAccount(DeployAccountTransactionOutput::default())
        }
        Transaction::Invoke(_) => TransactionOutput::Invoke(InvokeTransactionOutput::default()),
        Transaction::L1Handler(_) => {
            TransactionOutput::L1Handler(L1HandlerTransactionOutput::default())
        }
    }
}

fn set_events(tx: &mut TransactionOutput, events: Vec<Event>) {
    match tx {
        TransactionOutput::Declare(tx) => tx.events = events,
        TransactionOutput::Deploy(tx) => tx.events = events,
        TransactionOutput::DeployAccount(tx) => tx.events = events,
        TransactionOutput::Invoke(tx) => tx.events = events,
        TransactionOutput::L1Handler(tx) => tx.events = events,
    }
}

pub fn set_transaction_hash(tx: &mut Transaction, hash: TransactionHash) {
    match tx {
        Transaction::Declare(tx) => tx.transaction_hash = hash,
        Transaction::Deploy(tx) => tx.transaction_hash = hash,
        Transaction::DeployAccount(tx) => tx.transaction_hash = hash,
        Transaction::Invoke(tx) => tx.transaction_hash = hash,
        Transaction::L1Handler(tx) => tx.transaction_hash = hash,
    }
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
