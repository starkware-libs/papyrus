use std::collections::HashMap;

use starknet_api::core::{ContractAddress, EthAddress, PatriciaKey};
use starknet_api::hash::StarkFelt;
use starknet_api::transaction::{
    Builtin,
    DeclareTransactionOutput,
    DeployAccountTransactionOutput,
    DeployTransactionOutput,
    ExecutionResources,
    Fee,
    InvokeTransactionOutput,
    L1HandlerTransactionOutput,
    L2ToL1Payload,
    MessageToL1,
    RevertedTransactionExecutionStatus,
    TransactionExecutionStatus,
    TransactionOutput,
};

use super::common::try_from_starkfelt_to_u128;
use super::ProtobufConversionError;
use crate::protobuf;

impl TryFrom<protobuf::Receipt> for TransactionOutput {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Receipt) -> Result<Self, Self::Error> {
        let receipt = value
            .r#type
            .ok_or(ProtobufConversionError::MissingField { field_description: "Receipt::type" })?;
        match receipt {
            protobuf::receipt::Type::Invoke(invoke) => {
                Ok(TransactionOutput::Invoke(InvokeTransactionOutput::try_from(invoke)?))
            }
            protobuf::receipt::Type::L1Handler(l1_handler) => {
                Ok(TransactionOutput::L1Handler(L1HandlerTransactionOutput::try_from(l1_handler)?))
            }
            protobuf::receipt::Type::Declare(declare) => {
                Ok(TransactionOutput::Declare(DeclareTransactionOutput::try_from(declare)?))
            }
            protobuf::receipt::Type::DeprecatedDeploy(deploy) => {
                Ok(TransactionOutput::Deploy(DeployTransactionOutput::try_from(deploy)?))
            }
            protobuf::receipt::Type::DeployAccount(deploy_account) => {
                Ok(TransactionOutput::DeployAccount(DeployAccountTransactionOutput::try_from(
                    deploy_account,
                )?))
            }
        }
    }
}

// The output will have an empty events vec
impl TryFrom<protobuf::receipt::DeployAccount> for DeployAccountTransactionOutput {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::receipt::DeployAccount) -> Result<Self, Self::Error> {
        let (actual_fee, messages_sent, execution_status, execution_resources) =
            parse_common_receipt_fields(value.common)?;

        let events = vec![];

        let contract_address =
            value.contract_address.ok_or(ProtobufConversionError::MissingField {
                field_description: "DeployAccount::contract_address",
            })?;
        let felt = StarkFelt::try_from(contract_address)?;
        let contract_address = ContractAddress(PatriciaKey::try_from(felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "PatriciaKey",
                value_as_str: format!("{felt:?}"),
            }
        })?);

        Ok(Self {
            actual_fee,
            messages_sent,
            events,
            contract_address,
            execution_status,
            execution_resources,
        })
    }
}

// The output will have an empty events vec
impl TryFrom<protobuf::receipt::Deploy> for DeployTransactionOutput {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::receipt::Deploy) -> Result<Self, Self::Error> {
        let (actual_fee, messages_sent, execution_status, execution_resources) =
            parse_common_receipt_fields(value.common)?;

        let events = vec![];

        let contract_address =
            value.contract_address.ok_or(ProtobufConversionError::MissingField {
                field_description: "Deploy::contract_address",
            })?;
        let felt = StarkFelt::try_from(contract_address)?;
        let contract_address = ContractAddress(PatriciaKey::try_from(felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "PatriciaKey",
                value_as_str: format!("{felt:?}"),
            }
        })?);

        Ok(Self {
            actual_fee,
            messages_sent,
            events,
            contract_address,
            execution_status,
            execution_resources,
        })
    }
}

// The output will have an empty events vec
impl TryFrom<protobuf::receipt::Declare> for DeclareTransactionOutput {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::receipt::Declare) -> Result<Self, Self::Error> {
        let (actual_fee, messages_sent, execution_status, execution_resources) =
            parse_common_receipt_fields(value.common)?;

        let events = vec![];

        Ok(Self { actual_fee, messages_sent, events, execution_status, execution_resources })
    }
}

// The output will have an empty events vec
impl TryFrom<protobuf::receipt::Invoke> for InvokeTransactionOutput {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::receipt::Invoke) -> Result<Self, Self::Error> {
        let (actual_fee, messages_sent, execution_status, execution_resources) =
            parse_common_receipt_fields(value.common)?;

        let events = vec![];

        Ok(Self { actual_fee, messages_sent, events, execution_status, execution_resources })
    }
}

// The output will have an empty events vec
impl TryFrom<protobuf::receipt::L1Handler> for L1HandlerTransactionOutput {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::receipt::L1Handler) -> Result<Self, Self::Error> {
        let (actual_fee, messages_sent, execution_status, execution_resources) =
            parse_common_receipt_fields(value.common)?;

        let events = vec![];

        Ok(Self { actual_fee, messages_sent, events, execution_status, execution_resources })
    }
}

type ProtobufBuiltinCounter = protobuf::receipt::execution_resources::BuiltinCounter;

impl TryFrom<ProtobufBuiltinCounter> for HashMap<Builtin, u64> {
    type Error = ProtobufConversionError;
    fn try_from(value: ProtobufBuiltinCounter) -> Result<Self, Self::Error> {
        let mut builtin_instance_counter = HashMap::new();
        builtin_instance_counter.insert(Builtin::RangeCheck, u64::from(value.range_check));
        builtin_instance_counter.insert(Builtin::Pedersen, u64::from(value.pedersen));
        builtin_instance_counter.insert(Builtin::Poseidon, u64::from(value.poseidon));
        builtin_instance_counter.insert(Builtin::EcOp, u64::from(value.ec_op));
        builtin_instance_counter.insert(Builtin::Ecdsa, u64::from(value.ecdsa));
        builtin_instance_counter.insert(Builtin::Bitwise, u64::from(value.bitwise));
        builtin_instance_counter.insert(Builtin::Keccak, u64::from(value.keccak));
        builtin_instance_counter.insert(Builtin::SegmentArena, 0);
        Ok(builtin_instance_counter)
    }
}

impl From<HashMap<Builtin, u64>> for ProtobufBuiltinCounter {
    fn from(value: HashMap<Builtin, u64>) -> Self {
        let builtin_counter = ProtobufBuiltinCounter {
            range_check: u32::try_from(*value.get(&Builtin::RangeCheck).unwrap_or(&0))
                // TODO: should not panic
                .expect("Failed to convert u64 to u32"),
            pedersen: u32::try_from(*value.get(&Builtin::Pedersen).unwrap_or(&0))
                .expect("Failed to convert u64 to u32"),
            poseidon: u32::try_from(*value.get(&Builtin::Poseidon).unwrap_or(&0))
                .expect("Failed to convert u64 to u32"),
            ec_op: u32::try_from(*value.get(&Builtin::EcOp).unwrap_or(&0))
                .expect("Failed to convert u64 to u32"),
            ecdsa: u32::try_from(*value.get(&Builtin::Ecdsa).unwrap_or(&0))
                .expect("Failed to convert u64 to u32"),
            bitwise: u32::try_from(*value.get(&Builtin::Bitwise).unwrap_or(&0))
                .expect("Failed to convert u64 to u32"),
            keccak: u32::try_from(*value.get(&Builtin::Keccak).unwrap_or(&0))
                .expect("Failed to convert u64 to u32"),
            output: 0,
        };
        builtin_counter
    }
}

impl TryFrom<protobuf::receipt::ExecutionResources> for ExecutionResources {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::receipt::ExecutionResources) -> Result<Self, Self::Error> {
        let builtin_instance_counter = value
            .builtins
            .ok_or(ProtobufConversionError::MissingField { field_description: "builtins" })?;
        let builtin_instance_counter = HashMap::<Builtin, u64>::try_from(builtin_instance_counter)?;

        // TODO: remove all non-da gas consumed
        let da_l1_gas_consumed_felt =
            StarkFelt::try_from(value.l1_gas.ok_or(ProtobufConversionError::MissingField {
                field_description: "ExecutionResources::l1_gas",
            })?)?;
        let da_l1_gas_consumed = da_l1_gas_consumed_felt.try_into().map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u64",
                value_as_str: format!("{da_l1_gas_consumed_felt:?}"),
            }
        })?;

        let da_l1_data_gas_consumed_felt = StarkFelt::try_from(value.l1_data_gas.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "ExecutionResources::l1_data_gas",
            },
        )?)?;
        let da_l1_data_gas_consumed = da_l1_data_gas_consumed_felt.try_into().map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u64",
                value_as_str: format!("{da_l1_data_gas_consumed_felt:?}"),
            }
        })?;

        let execution_resources = ExecutionResources {
            steps: u64::from(value.steps),
            builtin_instance_counter,
            memory_holes: u64::from(value.memory_holes),
            da_l1_gas_consumed,
            da_l1_data_gas_consumed,
        };
        Ok(execution_resources)
    }
}

impl From<ExecutionResources> for protobuf::receipt::ExecutionResources {
    fn from(value: ExecutionResources) -> Self {
        let builtin_instance_counter = ProtobufBuiltinCounter::from(value.builtin_instance_counter);
        // TODO: add all l1 gas consumed, not just da
        let l1_gas = StarkFelt::from(value.da_l1_gas_consumed).into();
        let l1_data_gas = StarkFelt::from(value.da_l1_data_gas_consumed).into();
        // TODO: should not panic
        let steps = u32::try_from(value.steps).expect("Failed to convert u64 to u32");
        let memory_holes = u32::try_from(value.memory_holes).expect("Failed to convert u64 to u32");

        protobuf::receipt::ExecutionResources {
            builtins: Some(builtin_instance_counter),
            steps,
            memory_holes,
            l1_gas: Some(l1_gas),
            l1_data_gas: Some(l1_data_gas),
        }
    }
}

impl TryFrom<protobuf::EthereumAddress> for EthAddress {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::EthereumAddress) -> Result<Self, Self::Error> {
        let mut felt = [0; 20];
        if value.elements.len() != 20 {
            return Err(ProtobufConversionError::BytesDataLengthMismatch {
                type_description: "EthereumAddress",
                num_expected: 20,
                value: value.elements,
            });
        }
        felt.copy_from_slice(&value.elements);
        Ok(EthAddress(primitive_types::H160(felt)))
    }
}
impl From<EthAddress> for protobuf::EthereumAddress {
    fn from(value: EthAddress) -> Self {
        let elements = value.0.as_bytes().to_vec();
        protobuf::EthereumAddress { elements }
    }
}

impl TryFrom<protobuf::MessageToL1> for MessageToL1 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::MessageToL1) -> Result<Self, Self::Error> {
        let from_address_felt = StarkFelt::try_from(value.from_address.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "MessageToL1::from_address",
            },
        )?)?;
        let from_address = ContractAddress::try_from(from_address_felt)
            .expect("Converting ContractAddress from StarkFelt failed");

        let to_address = EthAddress::try_from(value.to_address.ok_or(
            ProtobufConversionError::MissingField { field_description: "MessageToL1::to_address" },
        )?)?;

        let payload = L2ToL1Payload(
            value.payload.into_iter().map(StarkFelt::try_from).collect::<Result<Vec<_>, _>>()?,
        );

        Ok(MessageToL1 { from_address, to_address, payload })
    }
}

impl From<MessageToL1> for protobuf::MessageToL1 {
    fn from(value: MessageToL1) -> Self {
        let from_address = StarkFelt::from(value.from_address).into();
        let to_address = value.to_address.into();
        let payload = value.payload.0.into_iter().map(protobuf::Felt252::from).collect();
        protobuf::MessageToL1 {
            from_address: Some(from_address),
            to_address: Some(to_address),
            payload,
        }
    }
}

fn parse_common_receipt_fields(
    common: Option<protobuf::receipt::Common>,
) -> Result<
    (Fee, Vec<MessageToL1>, TransactionExecutionStatus, ExecutionResources),
    ProtobufConversionError,
> {
    let common =
        common.ok_or(ProtobufConversionError::MissingField { field_description: "Common" })?;
    let actual_fee_felt =
        StarkFelt::try_from(common.actual_fee.ok_or(ProtobufConversionError::MissingField {
            field_description: "Common::actual_fee",
        })?)?;
    let actual_fee = Fee(try_from_starkfelt_to_u128(actual_fee_felt).map_err(|_| {
        ProtobufConversionError::OutOfRangeValue {
            type_description: "u128",
            value_as_str: format!("{actual_fee_felt:?}"),
        }
    })?);
    let messages_sent = common
        .messages_sent
        .into_iter()
        .map(MessageToL1::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    let execution_status =
        common.revert_reason.map_or(TransactionExecutionStatus::Succeeded, |revert_reason| {
            TransactionExecutionStatus::Reverted(RevertedTransactionExecutionStatus {
                revert_reason,
            })
        });
    let execution_resources = ExecutionResources::try_from(common.execution_resources.ok_or(
        ProtobufConversionError::MissingField { field_description: "Common::execution_resources" },
    )?)?;
    Ok((actual_fee, messages_sent, execution_status, execution_resources))
}
