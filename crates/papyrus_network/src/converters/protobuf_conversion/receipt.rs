use std::collections::HashMap;

use starknet_api::hash::StarkFelt;
use starknet_api::transaction::{Builtin, ExecutionResources};

use super::ProtobufConversionError;
use crate::protobuf_messages::protobuf::{self};

type ProtobufBuiltinCounter = protobuf::receipt::execution_resources::BuiltinCounter;

impl TryFrom<ProtobufBuiltinCounter> for HashMap<Builtin, u64> {
    type Error = ProtobufConversionError;
    fn try_from(value: ProtobufBuiltinCounter) -> Result<Self, Self::Error> {
        let mut builtin_instance_counter = HashMap::new();
        builtin_instance_counter.insert(Builtin::RangeCheck, value.range_check as u64);
        builtin_instance_counter.insert(Builtin::Pedersen, value.pedersen as u64);
        builtin_instance_counter.insert(Builtin::Poseidon, value.poseidon as u64);
        builtin_instance_counter.insert(Builtin::EcOp, value.ec_op as u64);
        builtin_instance_counter.insert(Builtin::Ecdsa, value.ecdsa as u64);
        builtin_instance_counter.insert(Builtin::Bitwise, value.bitwise as u64);
        builtin_instance_counter.insert(Builtin::Keccak, value.keccak as u64);
        builtin_instance_counter.insert(Builtin::SegmentArena, 0);
        Ok(builtin_instance_counter)
    }
}

impl From<HashMap<Builtin, u64>> for ProtobufBuiltinCounter {
    fn from(value: HashMap<Builtin, u64>) -> Self {
        let builtin_counter = ProtobufBuiltinCounter {
            range_check: *value.get(&Builtin::RangeCheck).unwrap_or(&0) as u32,
            pedersen: *value.get(&Builtin::Pedersen).unwrap_or(&0) as u32,
            poseidon: *value.get(&Builtin::Poseidon).unwrap_or(&0) as u32,
            ec_op: *value.get(&Builtin::EcOp).unwrap_or(&0) as u32,
            ecdsa: *value.get(&Builtin::Ecdsa).unwrap_or(&0) as u32,
            bitwise: *value.get(&Builtin::Bitwise).unwrap_or(&0) as u32,
            keccak: *value.get(&Builtin::Keccak).unwrap_or(&0) as u32,
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
            steps: value.steps as u64,
            builtin_instance_counter,
            memory_holes: value.memory_holes as u64,
            da_l1_gas_consumed,
            da_l1_data_gas_consumed,
        };
        Ok(execution_resources)
    }
}

impl From<ExecutionResources> for protobuf::receipt::ExecutionResources {
    fn from(value: ExecutionResources) -> Self {
        let builtin_instance_counter = ProtobufBuiltinCounter::from(value.builtin_instance_counter);
        let l1_gas = StarkFelt::from(value.da_l1_gas_consumed).into();
        let l1_data_gas = StarkFelt::from(value.da_l1_data_gas_consumed).into();

        protobuf::receipt::ExecutionResources {
            builtins: Some(builtin_instance_counter),
            steps: value.steps as u32,
            memory_holes: value.memory_holes as u32,
            l1_gas: Some(l1_gas),
            l1_data_gas: Some(l1_data_gas),
        }
    }
}
