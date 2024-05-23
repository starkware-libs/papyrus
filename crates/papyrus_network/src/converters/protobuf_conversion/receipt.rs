use std::collections::HashMap;

use starknet_api::core::{ContractAddress, EthAddress, PatriciaKey};
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::transaction::{
    Builtin,
    DeployAccountTransactionOutput,
    ExecutionResources,
    Fee,
    L2ToL1Payload,
    MessageToL1,
    RevertedTransactionExecutionStatus,
    TransactionExecutionStatus,
};

use super::ProtobufConversionError;
use crate::protobuf_messages::protobuf::{self};

// TODO: use the conversion in Starknet api once its upgraded
fn try_from_starkfelt_to_u128(felt: StarkFelt) -> Result<u128, &'static str> {
    const COMPLIMENT_OF_U128: usize = 16; // 32 - 16
    let (rest, u128_bytes) = felt.bytes().split_at(COMPLIMENT_OF_U128);
    if rest != [0u8; COMPLIMENT_OF_U128] {
        return Err("Value out of range");
    }

    let bytes: [u8; 16] = match u128_bytes.try_into() {
        Ok(b) => b,
        Err(_) => return Err("Failed to convert bytes to u128"),
    };

    Ok(u128::from_be_bytes(bytes))
}

impl TryFrom<protobuf::receipt::DeployAccount> for DeployAccountTransactionOutput {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::receipt::DeployAccount) -> Result<Self, Self::Error> {
        let common = value.common.clone().ok_or(ProtobufConversionError::MissingField {
            field_description: "DeployAccount::common",
        })?;

        let actual_fee_felt = StarkFelt::try_from(common.actual_fee.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "DeployAccount::common.actual_fee",
            },
        )?)?;
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

        let events = vec![];

        let Some(contract_address) = value.contract_address else {
            return Err(ProtobufConversionError::MissingField {
                field_description: "DeployAccount::contract_address",
            });
        };
        let mut felt = [0; 32];
        felt.copy_from_slice(&contract_address.elements);
        let contract_address = ContractAddress(
            // TODO: should not panic
            PatriciaKey::try_from(
                StarkHash::new(felt).expect("converting StarkHash from [u8; 32] failed"),
            )
            .expect("converting PatriciaKey from StarkHash failed"),
        );

        let execution_status = common.revert_reason.map_or_else(
            || TransactionExecutionStatus::Succeeded,
            |revert_reason| {
                TransactionExecutionStatus::Reverted(RevertedTransactionExecutionStatus {
                    revert_reason,
                })
            },
        );

        let execution_resources = ExecutionResources::try_from(common.execution_resources.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "DeployAccount::common.execution_resources",
            },
        )?)?;

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
