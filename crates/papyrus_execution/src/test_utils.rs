use std::collections::HashMap;

use blockifier::abi::abi_utils::get_storage_var_address;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use indexmap::indexmap;
use lazy_static::lazy_static;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::{StorageReader, StorageWriter};
use serde::de::DeserializeOwned;
use starknet_api::block::{BlockBody, BlockHeader, BlockNumber, BlockTimestamp, GasPrice};
use starknet_api::core::{
    ChainId,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    Nonce,
    PatriciaKey,
};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::state::{ContractClass, StateDiff, StateNumber};
use starknet_api::transaction::{
    Calldata,
    DeclareTransactionV0V1,
    DeclareTransactionV2,
    DeployAccountTransaction,
    Fee,
    InvokeTransaction,
    InvokeTransactionV1,
    TransactionVersion,
};
use starknet_api::{calldata, class_hash, contract_address, patricia_key, stark_felt};
use test_utils::read_json_file;

use crate::execution_utils::selector_from_name;
use crate::objects::TransactionTrace;
use crate::{simulate_transactions, ExecutableTransactionInput};

lazy_static! {
    pub static ref CHAIN_ID: ChainId = ChainId(String::from("TEST_CHAIN_ID"));
    pub static ref GAS_PRICE: GasPrice = GasPrice(100 * u128::pow(10, 9)); // Given in units of wei.
    pub static ref MAX_FEE: Fee = Fee(1000000 * GAS_PRICE.0);
    pub static ref BLOCK_TIMESTAMP: BlockTimestamp = BlockTimestamp(1234);
    pub static ref SEQUENCER_ADDRESS: ContractAddress = contract_address!("0xa");
    pub static ref DEPRECATED_CONTRACT_ADDRESS: ContractAddress = contract_address!("0x1");
    pub static ref CONTRACT_ADDRESS: ContractAddress = contract_address!("0x2");
    pub static ref ACCOUNT_CLASS_HASH: ClassHash = class_hash!("0x333");
    pub static ref ACCOUNT_ADDRESS: ContractAddress = contract_address!("0x444");
    // Taken from the trace of the deploy account transaction.
    pub static ref NEW_ACCOUNT_ADDRESS: ContractAddress =
        contract_address!("0x0153ade9ef510502c4f3b879c049dcc3ad5866706cae665f0d9df9b01e794fdb");
    pub static ref TEST_ERC20_CONTRACT_CLASS_HASH: ClassHash = class_hash!("0x1010");
    pub static ref TEST_ERC20_CONTRACT_ADDRESS: ContractAddress = contract_address!("0x1001");
    pub static ref ACCOUNT_INITIAL_BALANCE: StarkFelt = stark_felt!(2 * MAX_FEE.0);
}

fn get_test_instance<T: DeserializeOwned>(path_in_resource_dir: &str) -> T {
    serde_json::from_value(read_json_file(path_in_resource_dir)).unwrap()
}

// A deprecated class for testing, taken from get_deprecated_contract_class of Blockifier.
pub fn get_test_deprecated_contract_class() -> DeprecatedContractClass {
    get_test_instance("deprecated_class.json")
}
pub fn get_test_casm() -> CasmContractClass {
    get_test_instance("casm.json")
}
pub fn get_test_erc20_fee_contract_class() -> DeprecatedContractClass {
    get_test_instance("erc20_fee_contract_class.json")
}
// An account class for testing.
pub fn get_test_account_class() -> DeprecatedContractClass {
    get_test_instance("account_class.json")
}

pub fn prepare_storage(mut storage_writer: StorageWriter) {
    let class_hash0 = class_hash!("0x2");
    let class_hash1 = class_hash!("0x1");

    let minter_var_address = get_storage_var_address("permitted_minter", &[])
        .expect("Failed to get permitted_minter storage address.");

    let account_balance_key =
        get_storage_var_address("ERC20_balances", &[*ACCOUNT_ADDRESS.0.key()]).unwrap();
    let new_account_balance_key =
        get_storage_var_address("ERC20_balances", &[*NEW_ACCOUNT_ADDRESS.0.key()]).unwrap();

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(0),
            &BlockHeader {
                gas_price: *GAS_PRICE,
                sequencer: *SEQUENCER_ADDRESS,
                timestamp: *BLOCK_TIMESTAMP,
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(0), BlockBody::default())
        .unwrap()
        .append_state_diff(
            BlockNumber(0),
            StateDiff {
                deployed_contracts: indexmap!(
                    *TEST_ERC20_CONTRACT_ADDRESS => *TEST_ERC20_CONTRACT_CLASS_HASH,
                    *CONTRACT_ADDRESS => class_hash0,
                    *DEPRECATED_CONTRACT_ADDRESS => class_hash1,
                    *ACCOUNT_ADDRESS => *ACCOUNT_CLASS_HASH,
                ),
                storage_diffs: indexmap!(
                    *TEST_ERC20_CONTRACT_ADDRESS => indexmap!(
                        // Give the accounts some balance.
                        account_balance_key => *ACCOUNT_INITIAL_BALANCE,
                        new_account_balance_key => *ACCOUNT_INITIAL_BALANCE,
                        // Give the first account mint permission (what is this?).
                        minter_var_address => *ACCOUNT_ADDRESS.0.key()
                    ),
                ),
                declared_classes: indexmap!(
                    class_hash0 =>
                    // The class is not used in the execution, so it can be default.
                    (CompiledClassHash::default(), ContractClass::default())
                ),
                deprecated_declared_classes: indexmap!(
                    *TEST_ERC20_CONTRACT_CLASS_HASH => get_test_erc20_fee_contract_class(),
                    class_hash1 => get_test_deprecated_contract_class(),
                    *ACCOUNT_CLASS_HASH => get_test_account_class(),
                ),
                nonces: indexmap!(
                    *TEST_ERC20_CONTRACT_ADDRESS => Nonce::default(),
                    *CONTRACT_ADDRESS => Nonce::default(),
                    *DEPRECATED_CONTRACT_ADDRESS => Nonce::default(),
                    *ACCOUNT_ADDRESS => Nonce::default(),
                ),
                replaced_classes: indexmap!(),
            },
            indexmap!(),
        )
        .unwrap()
        .append_casm(&class_hash0, &get_test_casm())
        .unwrap()
        .commit()
        .unwrap();
}

pub fn execute_simulate_transactions(
    storage_reader: &StorageReader,
    txs: Vec<ExecutableTransactionInput>,
    charge_fee: bool,
    validate: bool,
) -> Vec<(TransactionTrace, GasPrice, Fee)> {
    let chain_id = ChainId(CHAIN_ID.to_string());
    let storage_txn = storage_reader.begin_ro_txn().unwrap();

    simulate_transactions(
        txs,
        &chain_id,
        &storage_txn,
        StateNumber::right_after_block(BlockNumber(0)),
        Some(*TEST_ERC20_CONTRACT_ADDRESS),
        charge_fee,
        validate,
    )
    .unwrap()
}

// Creates transactions for testing while resolving nonces and class hashes uniqueness.
pub struct TxsScenarioBuilder {
    // Each transaction by the same sender needs a unique nonce.
    sender_to_nonce: HashMap<ContractAddress, u128>,
    // Each declare class needs a unique class hash.
    next_class_hash: u128,
    // the result.
    txs: Vec<ExecutableTransactionInput>,
}

impl Default for TxsScenarioBuilder {
    fn default() -> Self {
        Self { sender_to_nonce: HashMap::new(), next_class_hash: 100_u128, txs: Vec::new() }
    }
}

impl TxsScenarioBuilder {
    pub fn collect(&self) -> Vec<ExecutableTransactionInput> {
        self.txs.clone()
    }

    pub fn invoke_deprecated(
        mut self,
        sender_address: ContractAddress,
        contract_address: ContractAddress,
        nonce: Option<Nonce>,
    ) -> Self {
        let calldata = calldata![
            *contract_address.0.key(),             // Contract address.
            selector_from_name("return_result").0, // EP selector.
            stark_felt!(1_u8),                     // Calldata length.
            stark_felt!(2_u8)                      // Calldata: num.
        ];
        let nonce = match nonce {
            None => self.next_nonce(sender_address),
            Some(nonce) => {
                let override_next_nonce: u128 =
                    u64::try_from(nonce.0).expect("Nonce should fit in u64.").into();
                self.sender_to_nonce.insert(sender_address, override_next_nonce + 1);
                nonce
            }
        };
        let tx = ExecutableTransactionInput::Invoke(InvokeTransaction::V1(InvokeTransactionV1 {
            calldata,
            max_fee: *MAX_FEE,
            sender_address,
            nonce,
            ..Default::default()
        }));
        self.txs.push(tx);
        self
    }

    pub fn declare_deprecated_class(mut self, sender_address: ContractAddress) -> Self {
        let tx = ExecutableTransactionInput::DeclareV1(
            DeclareTransactionV0V1 {
                max_fee: *MAX_FEE,
                sender_address,
                nonce: self.next_nonce(sender_address),
                class_hash: self.next_class_hash(),
                ..Default::default()
            },
            get_test_deprecated_contract_class(),
        );
        self.txs.push(tx);
        self
    }

    pub fn declare_class(mut self, sender_address: ContractAddress) -> TxsScenarioBuilder {
        let tx = ExecutableTransactionInput::DeclareV2(
            DeclareTransactionV2 {
                max_fee: *MAX_FEE,
                sender_address,
                nonce: self.next_nonce(sender_address),
                class_hash: self.next_class_hash(),
                ..Default::default()
            },
            get_test_casm(),
        );
        self.txs.push(tx);
        self
    }

    pub fn deploy_account(mut self) -> TxsScenarioBuilder {
        let tx = ExecutableTransactionInput::Deploy(DeployAccountTransaction {
            max_fee: *MAX_FEE,
            nonce: Nonce(stark_felt!(0_u128)),
            class_hash: *ACCOUNT_CLASS_HASH,
            version: TransactionVersion(1_u128.into()),
            ..Default::default()
        });
        self.txs.push(tx);
        self
    }

    fn next_nonce(&mut self, sender_address: ContractAddress) -> Nonce {
        match self.sender_to_nonce.get_mut(&sender_address) {
            Some(current) => {
                let res = Nonce(stark_felt!(*current));
                *current += 1;
                res
            }
            None => {
                self.sender_to_nonce.insert(sender_address, 1);
                Nonce(stark_felt!(0_u128))
            }
        }
    }

    fn next_class_hash(&mut self) -> ClassHash {
        let class_hash = ClassHash(self.next_class_hash.into());
        self.next_class_hash += 1;
        class_hash
    }
}
