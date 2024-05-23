use std::collections::HashMap;

use starknet_api::transaction::Builtin;

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
