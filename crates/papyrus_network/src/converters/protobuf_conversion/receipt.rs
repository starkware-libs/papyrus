use std::collections::HashMap;

use starknet_api::transaction::Builtin;

use super::ProtobufConversionError;
use crate::protobuf_messages::protobuf::{self};

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
