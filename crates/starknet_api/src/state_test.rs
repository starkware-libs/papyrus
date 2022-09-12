use std::env;
use std::fs::read_to_string;
use std::path::Path;
use std::string::String;

use super::{
    ContractClass, ContractNonce, DeclaredContract, DeployedContract, Program, StateDiff,
    StorageDiff,
};
use crate::{shash, ClassHash, ContractAddress, Nonce, StarkHash};

fn read_resource_file(path_in_resource_dir: &str) -> String {
    let path = Path::new(&env::current_dir().expect("Failed to find current directory."))
        .join("resources")
        .join(path_in_resource_dir);
    read_to_string(path.to_str().unwrap())
        .expect("Failed to read resource file.")
        .replace('\n', "")
        .replace(' ', "")
}

#[test]
fn state_sorted() {
    let hash0 = shash!("0x0");
    let hash1 = shash!("0x1");

    let dep_contract_0 = DeployedContract {
        address: ContractAddress::try_from(hash0).unwrap(),
        class_hash: ClassHash::new(hash0),
    };
    let dep_contract_1 = DeployedContract {
        address: ContractAddress::try_from(hash1).unwrap(),
        class_hash: ClassHash::new(hash1),
    };
    let storage_diff_0 =
        StorageDiff { address: ContractAddress::try_from(hash0).unwrap(), storage_entries: vec![] };
    let storage_diff_1 =
        StorageDiff { address: ContractAddress::try_from(hash1).unwrap(), storage_entries: vec![] };

    let dec_contract_0 = DeclaredContract {
        class_hash: ClassHash::new(hash0),
        contract_class: ContractClass::default(),
    };
    let dec_contract_1 = DeclaredContract {
        class_hash: ClassHash::new(hash1),
        contract_class: ContractClass::default(),
    };

    let nonce_0 = ContractNonce {
        contract_address: ContractAddress::try_from(hash0).unwrap(),
        nonce: Nonce::new(hash0),
    };
    let nonce_1 = ContractNonce {
        contract_address: ContractAddress::try_from(hash1).unwrap(),
        nonce: Nonce::new(hash1),
    };

    let unsorted_deployed_contracts = vec![dep_contract_1.clone(), dep_contract_0.clone()];
    let unsorted_storage_diffs = vec![storage_diff_1.clone(), storage_diff_0.clone()];
    let unsorted_declared_contracts = vec![dec_contract_1.clone(), dec_contract_0.clone()];
    let unsorted_nonces = vec![nonce_1.clone(), nonce_0.clone()];

    let state_diff = StateDiff::new(
        unsorted_deployed_contracts,
        unsorted_storage_diffs,
        unsorted_declared_contracts,
        unsorted_nonces,
    );

    let sorted_deployed_contracts = vec![dep_contract_0, dep_contract_1];
    let sorted_storage_diffs = vec![storage_diff_0, storage_diff_1];
    let sorted_declared_contracts = vec![dec_contract_0, dec_contract_1];
    let sorted_nonces = vec![nonce_0, nonce_1];

    assert_eq!(
        state_diff.destruct(),
        (sorted_deployed_contracts, sorted_storage_diffs, sorted_declared_contracts, sorted_nonces)
    );
}

#[test]
fn encode_decode_program() {
    let program: Program = serde_json::from_str(&read_resource_file("program.json"))
        .expect("Failed to serde program resource file.");

    let encoded = program.encode().unwrap();
    let decoded = Program::decode(encoded).unwrap();
    assert_eq!(program, decoded);
}
