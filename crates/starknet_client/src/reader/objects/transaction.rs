#[cfg(test)]
#[path = "transaction_test.rs"]
mod transaction_test;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
    Nonce,
};
use starknet_api::transaction::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    DeclareTransactionOutput,
    DeployAccountTransactionOutput,
    DeployTransactionOutput,
    Event,
    Fee,
    InvokeTransactionOutput,
    L1HandlerTransactionOutput,
    L1ToL2Payload,
    L2ToL1Payload,
    MessageToL1,
    PaymasterData,
    ResourceBoundsMapping,
    Tip,
    TransactionExecutionStatus,
    TransactionHash,
    TransactionOffsetInBlock,
    TransactionOutput,
    TransactionSignature,
    TransactionVersion,
};
use starknet_types_core::felt::Felt;

use crate::reader::ReaderClientError;

// TODO(dan): consider extracting common fields out (version, hash, type).
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum Transaction {
    #[serde(rename = "DECLARE")]
    Declare(IntermediateDeclareTransaction),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(IntermediateDeployAccountTransaction),
    #[serde(rename = "DEPLOY")]
    Deploy(DeployTransaction),
    #[serde(rename = "INVOKE_FUNCTION")]
    Invoke(IntermediateInvokeTransaction),
    #[serde(rename = "L1_HANDLER")]
    L1Handler(L1HandlerTransaction),
}

impl TryFrom<Transaction> for starknet_api::transaction::Transaction {
    type Error = ReaderClientError;
    fn try_from(tx: Transaction) -> Result<Self, ReaderClientError> {
        match tx {
            Transaction::Declare(declare_tx) => {
                Ok(starknet_api::transaction::Transaction::Declare(declare_tx.try_into()?))
            }
            Transaction::Deploy(deploy_tx) => {
                Ok(starknet_api::transaction::Transaction::Deploy(deploy_tx.into()))
            }
            Transaction::DeployAccount(deploy_acc_tx) => {
                Ok(starknet_api::transaction::Transaction::DeployAccount(deploy_acc_tx.try_into()?))
            }
            Transaction::Invoke(invoke_tx) => {
                Ok(starknet_api::transaction::Transaction::Invoke(invoke_tx.try_into()?))
            }
            Transaction::L1Handler(l1_handler_tx) => {
                Ok(starknet_api::transaction::Transaction::L1Handler(l1_handler_tx.into()))
            }
        }
    }
}

impl Transaction {
    pub fn transaction_hash(&self) -> TransactionHash {
        match self {
            Transaction::Declare(tx) => tx.transaction_hash,
            Transaction::Deploy(tx) => tx.transaction_hash,
            Transaction::DeployAccount(tx) => tx.transaction_hash,
            Transaction::Invoke(tx) => tx.transaction_hash,
            Transaction::L1Handler(tx) => tx.transaction_hash,
        }
    }

    pub fn transaction_hash_mut(&mut self) -> &mut TransactionHash {
        match self {
            Transaction::Declare(tx) => &mut tx.transaction_hash,
            Transaction::Deploy(tx) => &mut tx.transaction_hash,
            Transaction::DeployAccount(tx) => &mut tx.transaction_hash,
            Transaction::Invoke(tx) => &mut tx.transaction_hash,
            Transaction::L1Handler(tx) => &mut tx.transaction_hash,
        }
    }

    pub fn transaction_type(&self) -> TransactionType {
        match self {
            Transaction::Declare(_) => TransactionType::Declare,
            Transaction::Deploy(_) => TransactionType::Deploy,
            Transaction::DeployAccount(_) => TransactionType::DeployAccount,
            Transaction::Invoke(_) => TransactionType::InvokeFunction,
            Transaction::L1Handler(_) => TransactionType::L1Handler,
        }
    }

    pub fn contract_address(&self) -> Option<ContractAddress> {
        match self {
            Transaction::Deploy(tx) => Some(tx.contract_address),
            Transaction::DeployAccount(tx) => Some(tx.sender_address),
            _ => None,
        }
    }

    pub fn transaction_version(&self) -> TransactionVersion {
        match self {
            Transaction::Declare(tx) => tx.version,
            Transaction::Deploy(tx) => tx.version,
            Transaction::DeployAccount(tx) => tx.version,
            Transaction::Invoke(tx) => tx.version,
            Transaction::L1Handler(tx) => tx.version,
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct L1HandlerTransaction {
    pub transaction_hash: TransactionHash,
    pub version: TransactionVersion,
    #[serde(default)]
    pub nonce: Nonce,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
}

impl From<L1HandlerTransaction> for starknet_api::transaction::L1HandlerTransaction {
    fn from(l1_handler_tx: L1HandlerTransaction) -> Self {
        starknet_api::transaction::L1HandlerTransaction {
            version: l1_handler_tx.version,
            nonce: l1_handler_tx.nonce,
            contract_address: l1_handler_tx.contract_address,
            entry_point_selector: l1_handler_tx.entry_point_selector,
            calldata: l1_handler_tx.calldata,
        }
    }
}

// This enum is required since the FGW sends this field with value 0 as a reserved value. Once the
// feature will be activated this enum should be removed from here and taken from starknet-api.
#[derive(Debug, Deserialize_repr, Serialize_repr, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum ReservedDataAvailabilityMode {
    Reserved = 0,
}

impl From<ReservedDataAvailabilityMode> for starknet_api::data_availability::DataAvailabilityMode {
    fn from(_: ReservedDataAvailabilityMode) -> Self {
        starknet_api::data_availability::DataAvailabilityMode::L1
    }
}

// TODO(shahak, 01/11/2023): Add serde tests for v3 transactions.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IntermediateDeclareTransaction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_bounds: Option<ResourceBoundsMapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tip: Option<Tip>,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compiled_class_hash: Option<CompiledClassHash>,
    pub sender_address: ContractAddress,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce_data_availability_mode: Option<ReservedDataAvailabilityMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_data_availability_mode: Option<ReservedDataAvailabilityMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paymaster_data: Option<PaymasterData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_deployment_data: Option<AccountDeploymentData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_fee: Option<Fee>,
    pub version: TransactionVersion,
    pub transaction_hash: TransactionHash,
}

// TODO(shahak, 01/11/2023): Add conversion tests.
impl TryFrom<IntermediateDeclareTransaction> for starknet_api::transaction::DeclareTransaction {
    type Error = ReaderClientError;

    fn try_from(declare_tx: IntermediateDeclareTransaction) -> Result<Self, ReaderClientError> {
        if declare_tx.version == TransactionVersion::ZERO {
            Ok(Self::V0(declare_tx.try_into()?))
        } else if declare_tx.version == TransactionVersion::ONE {
            Ok(Self::V1(declare_tx.try_into()?))
        } else if declare_tx.version == TransactionVersion::TWO {
            Ok(Self::V2(declare_tx.try_into()?))
        } else if declare_tx.version == TransactionVersion::THREE {
            Ok(Self::V3(declare_tx.try_into()?))
        } else {
            Err(ReaderClientError::BadTransaction {
                tx_hash: declare_tx.transaction_hash,
                msg: format!("Declare version {:?} is not supported.", declare_tx.version),
            })
        }
    }
}

impl TryFrom<IntermediateDeclareTransaction> for starknet_api::transaction::DeclareTransactionV0V1 {
    type Error = ReaderClientError;

    fn try_from(declare_tx: IntermediateDeclareTransaction) -> Result<Self, ReaderClientError> {
        Ok(Self {
            max_fee: declare_tx.max_fee.ok_or(ReaderClientError::BadTransaction {
                tx_hash: declare_tx.transaction_hash,
                msg: "Declare V1 must contain max_fee field.".to_string(),
            })?,
            signature: declare_tx.signature,
            nonce: declare_tx.nonce,
            class_hash: declare_tx.class_hash,
            sender_address: declare_tx.sender_address,
        })
    }
}

impl TryFrom<IntermediateDeclareTransaction> for starknet_api::transaction::DeclareTransactionV2 {
    type Error = ReaderClientError;

    fn try_from(declare_tx: IntermediateDeclareTransaction) -> Result<Self, ReaderClientError> {
        Ok(Self {
            max_fee: declare_tx.max_fee.ok_or(ReaderClientError::BadTransaction {
                tx_hash: declare_tx.transaction_hash,
                msg: "Declare V2 must contain max_fee field.".to_string(),
            })?,
            signature: declare_tx.signature,
            nonce: declare_tx.nonce,
            class_hash: declare_tx.class_hash,
            compiled_class_hash: declare_tx.compiled_class_hash.ok_or(
                ReaderClientError::BadTransaction {
                    tx_hash: declare_tx.transaction_hash,
                    msg: "Declare V2 must contain compiled_class_hash field.".to_string(),
                },
            )?,
            sender_address: declare_tx.sender_address,
        })
    }
}

impl TryFrom<IntermediateDeclareTransaction> for starknet_api::transaction::DeclareTransactionV3 {
    type Error = ReaderClientError;

    fn try_from(declare_tx: IntermediateDeclareTransaction) -> Result<Self, ReaderClientError> {
        Ok(Self {
            resource_bounds: declare_tx.resource_bounds.ok_or(
                ReaderClientError::BadTransaction {
                    tx_hash: declare_tx.transaction_hash,
                    msg: "Declare V3 must contain resource_bounds field.".to_string(),
                },
            )?,
            tip: declare_tx.tip.ok_or(ReaderClientError::BadTransaction {
                tx_hash: declare_tx.transaction_hash,
                msg: "Declare V3 must contain tip field.".to_string(),
            })?,
            signature: declare_tx.signature,
            nonce: declare_tx.nonce,
            class_hash: declare_tx.class_hash,
            compiled_class_hash: declare_tx.compiled_class_hash.ok_or(
                ReaderClientError::BadTransaction {
                    tx_hash: declare_tx.transaction_hash,
                    msg: "Declare V3 must contain compiled_class_hash field.".to_string(),
                },
            )?,
            sender_address: declare_tx.sender_address,
            nonce_data_availability_mode: declare_tx
                .nonce_data_availability_mode
                .ok_or(ReaderClientError::BadTransaction {
                    tx_hash: declare_tx.transaction_hash,
                    msg: "Declare V3 must contain nonce_data_availability_mode field.".to_string(),
                })?
                .into(),
            fee_data_availability_mode: declare_tx
                .fee_data_availability_mode
                .ok_or(ReaderClientError::BadTransaction {
                    tx_hash: declare_tx.transaction_hash,
                    msg: "Declare V3 must contain fee_data_availability_mode field.".to_string(),
                })?
                .into(),
            paymaster_data: declare_tx.paymaster_data.ok_or(ReaderClientError::BadTransaction {
                tx_hash: declare_tx.transaction_hash,
                msg: "Declare V3 must contain paymaster_data field.".to_string(),
            })?,
            account_deployment_data: declare_tx.account_deployment_data.ok_or(
                ReaderClientError::BadTransaction {
                    tx_hash: declare_tx.transaction_hash,
                    msg: "Declare V3 must contain account_deployment_data field.".to_string(),
                },
            )?,
        })
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeployTransaction {
    pub contract_address: ContractAddress,
    pub contract_address_salt: ContractAddressSalt,
    pub class_hash: ClassHash,
    pub constructor_calldata: Calldata,
    pub transaction_hash: TransactionHash,
    #[serde(default)]
    pub version: TransactionVersion,
}

impl From<DeployTransaction> for starknet_api::transaction::DeployTransaction {
    fn from(deploy_tx: DeployTransaction) -> Self {
        starknet_api::transaction::DeployTransaction {
            version: deploy_tx.version,
            constructor_calldata: deploy_tx.constructor_calldata,
            class_hash: deploy_tx.class_hash,
            contract_address_salt: deploy_tx.contract_address_salt,
        }
    }
}

// TODO(shahak, 01/11/2023): Add serde tests for v3 transactions.
#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IntermediateDeployAccountTransaction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_bounds: Option<ResourceBoundsMapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tip: Option<Tip>,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce_data_availability_mode: Option<ReservedDataAvailabilityMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_data_availability_mode: Option<ReservedDataAvailabilityMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paymaster_data: Option<PaymasterData>,
    // In early versions of starknet, the `sender_address` field was originally named
    // `contract_address`.
    #[serde(alias = "contract_address")]
    pub sender_address: ContractAddress,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_fee: Option<Fee>,
    pub transaction_hash: TransactionHash,
    pub version: TransactionVersion,
}

// TODO(shahak, 01/11/2023): Add conversion tests.
impl TryFrom<IntermediateDeployAccountTransaction>
    for starknet_api::transaction::DeployAccountTransaction
{
    type Error = ReaderClientError;

    fn try_from(
        deploy_account_tx: IntermediateDeployAccountTransaction,
    ) -> Result<Self, ReaderClientError> {
        if deploy_account_tx.version == TransactionVersion::ONE {
            Ok(Self::V1(deploy_account_tx.try_into()?))
        } else if deploy_account_tx.version ==
            // Since v3 transactions, all transaction types are aligned with respect to the version,
            // v2 was skipped in the Deploy Account type.
            TransactionVersion::THREE
        {
            Ok(Self::V3(deploy_account_tx.try_into()?))
        } else {
            Err(ReaderClientError::BadTransaction {
                tx_hash: deploy_account_tx.transaction_hash,
                msg: format!(
                    "DeployAccount version {:?} is not supported.",
                    deploy_account_tx.version
                ),
            })
        }
    }
}

impl TryFrom<IntermediateDeployAccountTransaction>
    for starknet_api::transaction::DeployAccountTransactionV1
{
    type Error = ReaderClientError;

    fn try_from(
        deploy_account_tx: IntermediateDeployAccountTransaction,
    ) -> Result<Self, ReaderClientError> {
        Ok(Self {
            constructor_calldata: deploy_account_tx.constructor_calldata,
            class_hash: deploy_account_tx.class_hash,
            contract_address_salt: deploy_account_tx.contract_address_salt,
            max_fee: deploy_account_tx.max_fee.ok_or(ReaderClientError::BadTransaction {
                tx_hash: deploy_account_tx.transaction_hash,
                msg: "DeployAccount V1 must contain max_fee field.".to_string(),
            })?,
            signature: deploy_account_tx.signature,
            nonce: deploy_account_tx.nonce,
        })
    }
}

impl TryFrom<IntermediateDeployAccountTransaction>
    for starknet_api::transaction::DeployAccountTransactionV3
{
    type Error = ReaderClientError;

    fn try_from(
        deploy_account_tx: IntermediateDeployAccountTransaction,
    ) -> Result<Self, ReaderClientError> {
        Ok(Self {
            resource_bounds: deploy_account_tx.resource_bounds.ok_or(
                ReaderClientError::BadTransaction {
                    tx_hash: deploy_account_tx.transaction_hash,
                    msg: "DeployAccount V3 must contain resource_bounds field.".to_string(),
                },
            )?,
            tip: deploy_account_tx.tip.ok_or(ReaderClientError::BadTransaction {
                tx_hash: deploy_account_tx.transaction_hash,
                msg: "DeployAccount V3 must contain tip field.".to_string(),
            })?,
            signature: deploy_account_tx.signature,
            nonce: deploy_account_tx.nonce,
            class_hash: deploy_account_tx.class_hash,
            contract_address_salt: deploy_account_tx.contract_address_salt,
            constructor_calldata: deploy_account_tx.constructor_calldata,
            nonce_data_availability_mode: deploy_account_tx
                .nonce_data_availability_mode
                .ok_or(ReaderClientError::BadTransaction {
                    tx_hash: deploy_account_tx.transaction_hash,
                    msg: "DeployAccount V3 must contain nonce_data_availability_mode field."
                        .to_string(),
                })?
                .into(),
            fee_data_availability_mode: deploy_account_tx
                .fee_data_availability_mode
                .ok_or(ReaderClientError::BadTransaction {
                    tx_hash: deploy_account_tx.transaction_hash,
                    msg: "DeployAccount V3 must contain fee_data_availability_mode field."
                        .to_string(),
                })?
                .into(),
            paymaster_data: deploy_account_tx.paymaster_data.ok_or(
                ReaderClientError::BadTransaction {
                    tx_hash: deploy_account_tx.transaction_hash,
                    msg: "DeployAccount V3 must contain paymaster_data field.".to_string(),
                },
            )?,
        })
    }
}

// TODO(shahak, 01/11/2023): Add serde tests for v3 transactions.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IntermediateInvokeTransaction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_bounds: Option<ResourceBoundsMapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tip: Option<Tip>,
    pub calldata: Calldata,
    // In early versions of starknet, the `sender_address` field was originally named
    // `contract_address`.
    #[serde(alias = "contract_address")]
    pub sender_address: ContractAddress,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point_selector: Option<EntryPointSelector>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<Nonce>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_fee: Option<Fee>,
    pub signature: TransactionSignature,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce_data_availability_mode: Option<ReservedDataAvailabilityMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_data_availability_mode: Option<ReservedDataAvailabilityMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paymaster_data: Option<PaymasterData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_deployment_data: Option<AccountDeploymentData>,
    pub transaction_hash: TransactionHash,
    pub version: TransactionVersion,
}

// TODO(shahak, 01/11/2023): Add conversion tests.
impl TryFrom<IntermediateInvokeTransaction> for starknet_api::transaction::InvokeTransaction {
    type Error = ReaderClientError;

    fn try_from(invoke_tx: IntermediateInvokeTransaction) -> Result<Self, ReaderClientError> {
        if invoke_tx.version == TransactionVersion::ZERO {
            Ok(Self::V0(invoke_tx.try_into()?))
        } else if invoke_tx.version == TransactionVersion::ONE {
            Ok(Self::V1(invoke_tx.try_into()?))
        } else if invoke_tx.version ==
            // Since v3 transactions, all transaction types are aligned with respect to the version,
            // v2 was skipped in the Invoke type.
            TransactionVersion::THREE
        {
            Ok(Self::V3(invoke_tx.try_into()?))
        } else {
            Err(ReaderClientError::BadTransaction {
                tx_hash: invoke_tx.transaction_hash,
                msg: format!("Invoke version {:?} is not supported.", invoke_tx.version),
            })
        }
    }
}

impl TryFrom<IntermediateInvokeTransaction> for starknet_api::transaction::InvokeTransactionV0 {
    type Error = ReaderClientError;

    fn try_from(invoke_tx: IntermediateInvokeTransaction) -> Result<Self, ReaderClientError> {
        Ok(Self {
            max_fee: invoke_tx.max_fee.ok_or(ReaderClientError::BadTransaction {
                tx_hash: invoke_tx.transaction_hash,
                msg: "Invoke V0 must contain max_fee field.".to_string(),
            })?,
            signature: invoke_tx.signature,
            contract_address: invoke_tx.sender_address,
            entry_point_selector: invoke_tx.entry_point_selector.ok_or(
                ReaderClientError::BadTransaction {
                    tx_hash: invoke_tx.transaction_hash,
                    msg: "Invoke V0 must contain entry_point_selector field.".to_string(),
                },
            )?,
            calldata: invoke_tx.calldata,
        })
    }
}

impl TryFrom<IntermediateInvokeTransaction> for starknet_api::transaction::InvokeTransactionV1 {
    type Error = ReaderClientError;

    fn try_from(invoke_tx: IntermediateInvokeTransaction) -> Result<Self, ReaderClientError> {
        // TODO(yair): Consider asserting that entry_point_selector is None.
        Ok(Self {
            max_fee: invoke_tx.max_fee.ok_or(ReaderClientError::BadTransaction {
                tx_hash: invoke_tx.transaction_hash,
                msg: "Invoke V1 must contain max_fee field.".to_string(),
            })?,
            signature: invoke_tx.signature,
            nonce: invoke_tx.nonce.ok_or(ReaderClientError::BadTransaction {
                tx_hash: invoke_tx.transaction_hash,
                msg: "Invoke V1 must contain nonce field.".to_string(),
            })?,
            sender_address: invoke_tx.sender_address,
            calldata: invoke_tx.calldata,
        })
    }
}

impl TryFrom<IntermediateInvokeTransaction> for starknet_api::transaction::InvokeTransactionV3 {
    type Error = ReaderClientError;

    fn try_from(invoke_tx: IntermediateInvokeTransaction) -> Result<Self, ReaderClientError> {
        // TODO(yair): Consider asserting that entry_point_selector is None.
        Ok(Self {
            resource_bounds: invoke_tx.resource_bounds.ok_or(
                ReaderClientError::BadTransaction {
                    tx_hash: invoke_tx.transaction_hash,
                    msg: "Invoke V3 must contain resource_bounds field.".to_string(),
                },
            )?,
            tip: invoke_tx.tip.ok_or(ReaderClientError::BadTransaction {
                tx_hash: invoke_tx.transaction_hash,
                msg: "Invoke V3 must contain tip field.".to_string(),
            })?,
            signature: invoke_tx.signature,
            nonce: invoke_tx.nonce.ok_or(ReaderClientError::BadTransaction {
                tx_hash: invoke_tx.transaction_hash,
                msg: "Invoke V3 must contain nonce field.".to_string(),
            })?,
            sender_address: invoke_tx.sender_address,
            calldata: invoke_tx.calldata,
            nonce_data_availability_mode: invoke_tx
                .nonce_data_availability_mode
                .ok_or(ReaderClientError::BadTransaction {
                    tx_hash: invoke_tx.transaction_hash,
                    msg: "Invoke V3 must contain nonce_data_availability_mode field.".to_string(),
                })?
                .into(),
            fee_data_availability_mode: invoke_tx
                .fee_data_availability_mode
                .ok_or(ReaderClientError::BadTransaction {
                    tx_hash: invoke_tx.transaction_hash,
                    msg: "Invoke V3 must contain fee_data_availability_mode field.".to_string(),
                })?
                .into(),
            paymaster_data: invoke_tx.paymaster_data.ok_or(ReaderClientError::BadTransaction {
                tx_hash: invoke_tx.transaction_hash,
                msg: "Invoke V3 must contain paymaster_data field.".to_string(),
            })?,
            account_deployment_data: invoke_tx.account_deployment_data.ok_or(
                ReaderClientError::BadTransaction {
                    tx_hash: invoke_tx.transaction_hash,
                    msg: "Invoke V3 must contain account_deployment_data field.".to_string(),
                },
            )?,
        })
    }
}

/// The execution resources used by a transaction.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct ExecutionResources {
    // Note: in starknet_api this field is named `steps`
    pub n_steps: u64,
    pub builtin_instance_counter: HashMap<Builtin, u64>,
    // Note: in starknet_api this field is named `memory_holes`
    pub n_memory_holes: u64,
}

// Note: the serialization is different from the one in starknet_api.
#[derive(Hash, Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum Builtin {
    #[serde(rename = "range_check_builtin")]
    RangeCheck,
    #[serde(rename = "pedersen_builtin")]
    Pedersen,
    #[serde(rename = "poseidon_builtin")]
    Poseidon,
    #[serde(rename = "ec_op_builtin")]
    EcOp,
    #[serde(rename = "ecdsa_builtin")]
    Ecdsa,
    #[serde(rename = "bitwise_builtin")]
    Bitwise,
    #[serde(rename = "keccak_builtin")]
    Keccak,
    // Note: in starknet_api this variant doesn't exist.
    #[serde(rename = "output_builtin")]
    Output,
    #[serde(rename = "segment_arena_builtin")]
    SegmentArena,
}

impl From<ExecutionResources> for starknet_api::transaction::ExecutionResources {
    fn from(execution_resources: ExecutionResources) -> Self {
        Self {
            steps: execution_resources.n_steps,
            builtin_instance_counter: execution_resources
                .builtin_instance_counter
                .into_iter()
                .filter_map(|(builtin, count)| match builtin {
                    Builtin::RangeCheck => {
                        Some((starknet_api::transaction::Builtin::RangeCheck, count))
                    }
                    Builtin::Pedersen => {
                        Some((starknet_api::transaction::Builtin::Pedersen, count))
                    }
                    Builtin::Poseidon => {
                        Some((starknet_api::transaction::Builtin::Poseidon, count))
                    }
                    Builtin::EcOp => Some((starknet_api::transaction::Builtin::EcOp, count)),
                    Builtin::Ecdsa => Some((starknet_api::transaction::Builtin::Ecdsa, count)),
                    Builtin::Bitwise => Some((starknet_api::transaction::Builtin::Bitwise, count)),
                    Builtin::Keccak => Some((starknet_api::transaction::Builtin::Keccak, count)),
                    // output builtin should be ignored.
                    Builtin::Output => None,
                    Builtin::SegmentArena => {
                        Some((starknet_api::transaction::Builtin::SegmentArena, count))
                    }
                })
                .collect(),
            memory_holes: execution_resources.n_memory_holes,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct TransactionReceipt {
    pub transaction_index: TransactionOffsetInBlock,
    pub transaction_hash: TransactionHash,
    #[serde(default)]
    pub l1_to_l2_consumed_message: L1ToL2Message,
    pub l2_to_l1_messages: Vec<L2ToL1Message>,
    pub events: Vec<Event>,
    #[serde(default)]
    pub execution_resources: ExecutionResources,
    pub actual_fee: Fee,
    #[serde(default)]
    pub execution_status: TransactionExecutionStatus,
}

impl TransactionReceipt {
    pub fn into_starknet_api_transaction_output(
        self,
        transaction: &Transaction,
    ) -> TransactionOutput {
        let messages_sent = self.l2_to_l1_messages.into_iter().map(MessageToL1::from).collect();
        let contract_address = transaction.contract_address();
        match transaction.transaction_type() {
            TransactionType::Declare => TransactionOutput::Declare(DeclareTransactionOutput {
                actual_fee: self.actual_fee,
                messages_sent,
                events: self.events,
                execution_status: self.execution_status,
                execution_resources: self.execution_resources.into(),
            }),
            TransactionType::Deploy => TransactionOutput::Deploy(DeployTransactionOutput {
                actual_fee: self.actual_fee,
                messages_sent,
                events: self.events,
                contract_address: contract_address
                    .expect("Deploy transaction must have a contract address."),
                execution_status: self.execution_status,
                execution_resources: self.execution_resources.into(),
            }),
            TransactionType::DeployAccount => {
                TransactionOutput::DeployAccount(DeployAccountTransactionOutput {
                    actual_fee: self.actual_fee,
                    messages_sent,
                    events: self.events,
                    contract_address: contract_address
                        .expect("Deploy account transaction must have a contract address."),
                    execution_status: self.execution_status,
                    execution_resources: self.execution_resources.into(),
                })
            }
            TransactionType::InvokeFunction => TransactionOutput::Invoke(InvokeTransactionOutput {
                actual_fee: self.actual_fee,
                messages_sent,
                events: self.events,
                execution_status: self.execution_status,
                execution_resources: self.execution_resources.into(),
            }),
            TransactionType::L1Handler => {
                TransactionOutput::L1Handler(L1HandlerTransactionOutput {
                    actual_fee: self.actual_fee,
                    messages_sent,
                    events: self.events,
                    execution_status: self.execution_status,
                    execution_resources: self.execution_resources.into(),
                })
            }
        }
    }
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1ToL2Nonce(pub Felt);

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct L1ToL2Message {
    pub from_address: EthAddress,
    pub to_address: ContractAddress,
    pub selector: EntryPointSelector,
    pub payload: L1ToL2Payload,
    #[serde(default)]
    pub nonce: L1ToL2Nonce,
}

impl From<L1ToL2Message> for starknet_api::transaction::MessageToL2 {
    fn from(message: L1ToL2Message) -> Self {
        starknet_api::transaction::MessageToL2 {
            from_address: message.from_address,
            payload: message.payload,
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct L2ToL1Message {
    pub from_address: ContractAddress,
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

impl From<L2ToL1Message> for starknet_api::transaction::MessageToL1 {
    fn from(message: L2ToL1Message) -> Self {
        starknet_api::transaction::MessageToL1 {
            to_address: message.to_address,
            payload: message.payload,
            from_address: message.from_address,
        }
    }
}

#[derive(
    Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default,
)]
pub enum TransactionType {
    #[serde(rename(deserialize = "DECLARE", serialize = "DECLARE"))]
    Declare,
    #[serde(rename(deserialize = "DEPLOY", serialize = "DEPLOY"))]
    Deploy,
    #[serde(rename(deserialize = "DEPLOY_ACCOUNT", serialize = "DEPLOY_ACCOUNT"))]
    DeployAccount,
    #[serde(rename(deserialize = "INVOKE_FUNCTION", serialize = "INVOKE_FUNCTION"))]
    #[default]
    InvokeFunction,
    #[serde(rename(deserialize = "L1_HANDLER", serialize = "L1_HANDLER"))]
    L1Handler,
}
