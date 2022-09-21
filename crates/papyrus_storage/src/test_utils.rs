use starknet_api::{
    shash, Block, BlockBody, BlockHash, BlockHeader, BlockNumber, CallData, ClassHash,
    ContractAddress, ContractAddressSalt, ContractClass, ContractNonce, DeclaredContract,
    DeployTransaction, DeployTransactionOutput, DeployedContract, Fee, GlobalRoot, Nonce,
    StarkHash, StorageDiff, StorageEntry, StorageKey, Transaction, TransactionHash,
    TransactionOutput, TransactionVersion,
};
use tempfile::tempdir;

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
            messages_sent: vec![],
            events: vec![],
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

pub fn get_test_state_diff()
-> (BlockHeader, BlockHeader, starknet_api::StateDiff, Vec<DeclaredContract>) {
    let parent_hash =
        BlockHash::new(shash!("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5483"));
    let state_root = GlobalRoot::new(shash!("0x12"));
    let parent_header = BlockHeader {
        block_number: BlockNumber::new(0),
        block_hash: parent_hash,
        state_root,
        ..BlockHeader::default()
    };

    let block_hash =
        BlockHash::new(shash!("0x642b629ad8ce233b55798c83bb629a59bf0a0092f67da28d6d66776680d5493"));
    let header = BlockHeader {
        block_number: BlockNumber::new(1),
        block_hash,
        parent_hash,
        ..BlockHeader::default()
    };

    let address0 = ContractAddress::try_from(shash!("0x11")).unwrap();
    let hash0 = ClassHash::new(shash!("0x4"));
    let address1 = ContractAddress::try_from(shash!("0x21")).unwrap();
    let hash1 = ClassHash::new(shash!("0x5"));
    let class0 = ContractClass::default();
    let class1 = ContractClass::default();
    let key0 = StorageKey::try_from(shash!("0x1001")).unwrap();
    let value0 = shash!("0x200");
    let key1 = StorageKey::try_from(shash!("0x1002")).unwrap();
    let value1 = shash!("0x201");
    let diff = starknet_api::StateDiff::new(
        vec![
            DeployedContract { address: address0, class_hash: hash0 },
            DeployedContract { address: address1, class_hash: hash1 },
        ],
        vec![
            StorageDiff {
                address: address0,
                storage_entries: vec![
                    StorageEntry { key: key0.clone(), value: value0 },
                    StorageEntry { key: key1, value: value1 },
                ],
            },
            StorageDiff {
                address: address1,
                storage_entries: vec![StorageEntry { key: key0, value: value0 }],
            },
        ],
        vec![
            DeclaredContract { class_hash: hash0, contract_class: class0 },
            DeclaredContract { class_hash: hash1, contract_class: class1 },
        ],
        vec![
            ContractNonce { contract_address: address0, nonce: Nonce::new(StarkHash::from_u64(1)) },
            ContractNonce { contract_address: address1, nonce: Nonce::new(StarkHash::from_u64(1)) },
        ],
    )
    .unwrap();

    (parent_header, header, diff, vec![])
}
