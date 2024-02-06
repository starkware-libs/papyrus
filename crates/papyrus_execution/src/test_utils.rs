use std::collections::HashMap;

use blockifier::abi::abi_utils::get_storage_var_address;
use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use indexmap::indexmap;
use lazy_static::lazy_static;
use num_traits::ToPrimitive;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::{StorageReader, StorageWriter};
use serde::de::DeserializeOwned;
use starknet_api::block::{
    BlockBody,
    BlockHash,
    BlockHeader,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
};
use starknet_api::core::{
    ChainId,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    Nonce,
    PatriciaKey,
};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{ContractClass, StateDiff, StateNumber};
use starknet_api::transaction::{
    Calldata,
    DeclareTransactionV0V1,
    DeclareTransactionV2,
    DeployAccountTransaction,
    DeployAccountTransactionV1,
    Fee,
    InvokeTransaction,
    InvokeTransactionV1,
    TransactionHash,
};
use starknet_api::{calldata, class_hash, contract_address, patricia_key};
use starknet_types_core::felt::Felt;
use test_utils::read_json_file;

use crate::execution_utils::selector_from_name;
use crate::objects::{PendingData, TransactionSimulationOutput};
use crate::testing_instances::test_block_execution_config;
use crate::{simulate_transactions, ExecutableTransactionInput, OnlyQuery};

lazy_static! {
    pub static ref CHAIN_ID: ChainId = ChainId(String::from("TEST_CHAIN_ID"));
    pub static ref GAS_PRICE: GasPrice = GasPrice(100 * u128::pow(10, 9)); // Given in units of wei.
    pub static ref MAX_FEE: Fee = Fee(1000000 * GAS_PRICE.0);
    pub static ref BLOCK_TIMESTAMP: BlockTimestamp = BlockTimestamp(1234);
    pub static ref SEQUENCER_ADDRESS: ContractAddress = contract_address!(0xa);
    pub static ref DEPRECATED_CONTRACT_ADDRESS: ContractAddress = contract_address!(0x1);
    pub static ref CONTRACT_ADDRESS: ContractAddress = contract_address!(0x2);
    pub static ref ACCOUNT_CLASS_HASH: ClassHash = class_hash!(0x333);
    pub static ref ACCOUNT_ADDRESS: ContractAddress = contract_address!(0x444);
    // Taken from the trace of the deploy account transaction.
    pub static ref NEW_ACCOUNT_ADDRESS: ContractAddress =
        contract_address!(Felt::from_hex("0x0153ade9ef510502c4f3b879c049dcc3ad5866706cae665f0d9df9b01e794fdb").unwrap());
    pub static ref TEST_ERC20_CONTRACT_CLASS_HASH: ClassHash = class_hash!(0x1010);
    pub static ref TEST_ERC20_CONTRACT_ADDRESS: ContractAddress = contract_address!(0x1001);
    pub static ref ACCOUNT_INITIAL_BALANCE: Felt = Felt::from(2 * MAX_FEE.0);
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
    let class_hash0 = class_hash!(0x2);
    let class_hash1 = class_hash!(0x1);

    let minter_var_address = get_storage_var_address("permitted_minter", &[]);

    let account_balance_key =
        get_storage_var_address("ERC20_balances", &[ACCOUNT_ADDRESS.0.to_felt()]);
    let new_account_balance_key =
        get_storage_var_address("ERC20_balances", &[NEW_ACCOUNT_ADDRESS.0.to_felt()]);

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(
            BlockNumber(0),
            &BlockHeader {
                eth_l1_gas_price: *GAS_PRICE,
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
                        minter_var_address => ACCOUNT_ADDRESS.0.to_felt()
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
        .append_header(
            BlockNumber(1),
            &BlockHeader {
                eth_l1_gas_price: *GAS_PRICE,
                sequencer: *SEQUENCER_ADDRESS,
                timestamp: *BLOCK_TIMESTAMP,
                block_hash: BlockHash(Felt::ONE),
                parent_hash: BlockHash(Felt::ZERO),
                ..Default::default()
            },
        )
        .unwrap()
        .append_body(BlockNumber(1), BlockBody::default())
        .unwrap()
        .append_state_diff(BlockNumber(1), StateDiff::default(), indexmap![])
        .unwrap()
        .commit()
        .unwrap();
}

pub fn execute_simulate_transactions(
    storage_reader: StorageReader,
    maybe_pending_data: Option<PendingData>,
    txs: Vec<ExecutableTransactionInput>,
    tx_hashes: Option<Vec<TransactionHash>>,
    charge_fee: bool,
    validate: bool,
) -> Vec<TransactionSimulationOutput> {
    let chain_id = ChainId(CHAIN_ID.to_string());

    simulate_transactions(
        txs,
        tx_hashes,
        &chain_id,
        storage_reader,
        maybe_pending_data,
        StateNumber::right_after_block(BlockNumber(0)),
        BlockNumber(1),
        &test_block_execution_config(),
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
        only_query: OnlyQuery,
    ) -> Self {
        let calldata = calldata![
            contract_address.0.to_felt(),          // Contract address.
            selector_from_name("return_result").0, // EP selector.
            Felt::ONE,                             // Calldata length.
            Felt::TWO                              // Calldata: num.
        ];
        let nonce = match nonce {
            None => self.next_nonce(sender_address),
            Some(nonce) => {
                let override_next_nonce: u128 =
                    nonce.0.to_u64().expect("Nonce should fit in u64.").into();
                self.sender_to_nonce.insert(sender_address, override_next_nonce + 1);
                nonce
            }
        };
        let tx = ExecutableTransactionInput::Invoke(
            InvokeTransaction::V1(InvokeTransactionV1 {
                calldata,
                max_fee: *MAX_FEE,
                sender_address,
                nonce,
                ..Default::default()
            }),
            only_query,
        );
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
            false,
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
            false,
        );
        self.txs.push(tx);
        self
    }

    pub fn deploy_account(mut self) -> TxsScenarioBuilder {
        let tx = ExecutableTransactionInput::DeployAccount(
            DeployAccountTransaction::V1(DeployAccountTransactionV1 {
                max_fee: *MAX_FEE,
                nonce: Nonce(Felt::ZERO),
                class_hash: *ACCOUNT_CLASS_HASH,
                ..Default::default()
            }),
            false,
        );
        self.txs.push(tx);
        self
    }

    // TODO(yair): add l1 handler transaction.

    fn next_nonce(&mut self, sender_address: ContractAddress) -> Nonce {
        match self.sender_to_nonce.get_mut(&sender_address) {
            Some(current) => {
                let res = Nonce(Felt::from(*current));
                *current += 1;
                res
            }
            None => {
                self.sender_to_nonce.insert(sender_address, 1);
                Nonce(Felt::ZERO)
            }
        }
    }

    fn next_class_hash(&mut self) -> ClassHash {
        let class_hash = ClassHash(self.next_class_hash.into());
        self.next_class_hash += 1;
        class_hash
    }
}
