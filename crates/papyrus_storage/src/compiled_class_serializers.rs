use byteorder::BigEndian;
use cairo_lang_casm::hints::{CoreHint, CoreHintBase, DeprecatedHint, Hint, StarknetHint};
use cairo_lang_casm::operand::{
    BinOpOperand, CellRef, DerefOrImmediate, Operation, Register, ResOperand,
};
use cairo_lang_starknet::casm_contract_class::{
    CasmContractClass, CasmContractEntryPoint, CasmContractEntryPoints,
};
use cairo_lang_utils::bigint::{BigIntAsHex, BigUintAsHex};
use num_bigint::{BigInt, BigUint, Sign};

use crate::compiled_class_serializers_helper::*;
use crate::db::serialization::{StorageSerde, StorageSerdeError};
#[cfg(test)]
use crate::serializers::serializers_test::{create_storage_serde_test, StorageSerdeTest};
use crate::serializers::*;

auto_storage_serde! {
    pub struct CasmContractClass {
        pub prime: BigUint,
        pub compiler_version: String,
        pub bytecode: Vec<BigUintAsHex>,
        pub hints: Vec<(usize, Vec<Hint>)>,
        pub pythonic_hints: Option<Vec<(usize, Vec<String>)>>,
        pub entry_points_by_type: CasmContractEntryPoints,
    }

    pub enum Hint {
        Core(CoreHintBase) = 0,
        Starknet(StarknetHint) = 1,
    }

    pub struct CasmContractEntryPoints {
        pub external: Vec<CasmContractEntryPoint>,
        pub l1_handler: Vec<CasmContractEntryPoint>,
        pub constructor: Vec<CasmContractEntryPoint>,
    }

    pub enum CoreHintBase {
        Core(CoreHint) = 0,
        Deprecated(DeprecatedHint) = 1,
    }

    pub struct CasmContractEntryPoint {
        pub selector: BigUint,
        pub offset: usize,
        pub builtins: Vec<String>,
    }

    pub struct CellRef {
        pub register: Register,
        pub offset: i16,
    }
    pub enum Register {
        AP = 0,
        FP = 1,
    }

    pub struct BigUintAsHex {
        pub value: BigUint,
    }

    pub struct BigIntAsHex {
        pub value: BigInt,
    }

    pub enum Sign {
        Minus = 0,
        NoSign = 1,
        Plus = 2,
    }

    pub struct BinOpOperand {
        pub op: Operation,
        pub a: CellRef,
        pub b: DerefOrImmediate,
    }

    pub enum Operation {
        Add = 0,
        Mul = 1,
    }
    pub enum DerefOrImmediate {
        Deref(CellRef) = 0,
        Immediate(BigIntAsHex) = 1,
    }

    binary(i16, read_i16, write_i16);
    (usize, Vec<Hint>);

    // Helper structs
    // TODO(yair): Remove them once the macro supports enums with named variables and tuples.
    pub enum CoreHintHelper {
        AllocSegment(AllocSegmentStruct) = 0,
        TestLessThan(TestLessThanStruct) = 1,
        TestLessThanOrEqual(TestLessThanOrEqualStruct) = 2,
        WideMul128(WideMul128Struct) = 3,
        DivMod(DivModStruct) = 4,
        Uint256DivMod(Uint256DivModStruct) = 5,
        SquareRoot(SquareRootStruct) = 6,
        Uint256SquareRoot(Uint256SquareRootStruct) = 7,
        LinearSplit(LinearSplitStruct) = 8,
        AllocFelt252Dict(AllocFelt252DictStruct) = 9,
        Felt252DictEntryInit(Felt252DictEntryInitStruct) = 10,
        Felt252DictEntryUpdate(Felt252DictEntryUpdateStruct) = 11,
        GetSegmentArenaIndex(GetSegmentArenaIndexStruct) = 12,
        InitSquashData(InitSquashDataStruct) = 13,
        GetCurrentAccessIndex(GetCurrentAccessIndexStruct) = 14,
        ShouldSkipSquashLoop(ShouldSkipSquashLoopStruct) = 15,
        GetCurrentAccessDelta(GetCurrentAccessDeltaStruct) = 16,
        ShouldContinueSquashLoop(ShouldContinueSquashLoopStruct) = 17,
        GetNextDictKey(GetNextDictKeyStruct) = 18,
        AssertLeFindSmallArcs(AssertLeFindSmallArcsStruct) = 19,
        AssertLeIsFirstArcExcluded(AssertLeIsFirstArcExcludedStruct) = 20,
        AssertLeIsSecondArcExcluded(AssertLeIsSecondArcExcludedStruct) = 21,
        RandomEcPoint(RandomEcPointStruct) = 22,
        FieldSqrt(FieldSqrtStruct) = 23,
        DebugPrint(DebugPrintStruct) = 24,
        AllocConstantSize(AllocConstantSizeStruct) = 25,
        Uint512DivModByUint256(Uint512DivModByUint256Struct) = 26,
    }

    pub enum ResOperandHelper {
        Deref(CellRef) = 0,
        DoubleDeref(DoubleDerefStruct) = 1,
        Immediate(BigIntAsHex) = 2,
        BinOp(BinOpOperand) = 3,
    }

    pub enum DeprecatedHintHelper {
        AssertCurrentAccessIndicesIsEmpty = 0,
        AssertAllAccessesUsed(AssertAllAccessesUsedStruct) = 1,
        AssertAllKeysUsed = 2,
        AssertLeAssertThirdArcExcluded = 3,
        AssertLtAssertValidInput(AssertLtAssertValidInputStruct) = 4,
        Felt252DictRead(Felt252DictReadStruct) = 5,
        Felt252DictWrite(Felt252DictWriteStruct) = 6,
    }

    pub enum StarknetHintHelper {
        SystemCall(ResOperand) = 0,
        SetBlockNumber(ResOperand) = 1,
        SetBlockTimestamp(ResOperand) = 2,
        SetCallerAddress(ResOperand) = 3,
        SetContractAddress(ResOperand) = 4,
        SetSequencerAddress(ResOperand) = 5,
        SetVersion(ResOperand) = 6,
        SetAccountContractAddress(ResOperand) = 7,
        SetMaxFee(ResOperand) = 8,
        SetTransactionHash(ResOperand) = 9,
        SetChainId(ResOperand) = 10,
        SetNonce(ResOperand) = 11,
        SetSignature(SetSignatureStruct) = 12,
    }

    pub struct DoubleDerefStruct(pub CellRef, pub i16);

    pub struct AllocSegmentStruct {
        pub dst: CellRef,
    }
    pub struct TestLessThanStruct {
        pub lhs: ResOperand,
        pub rhs: ResOperand,
        pub dst: CellRef,
    }
    pub struct TestLessThanOrEqualStruct {
        pub lhs: ResOperand,
        pub rhs: ResOperand,
        pub dst: CellRef,
    }
    pub struct WideMul128Struct {
        pub lhs: ResOperand,
        pub rhs: ResOperand,
        pub high: CellRef,
        pub low: CellRef,
    }
    pub struct DivModStruct {
        pub lhs: ResOperand,
        pub rhs: ResOperand,
        pub quotient: CellRef,
        pub remainder: CellRef,
    }
    pub struct Uint256DivModStruct {
        pub dividend_low: ResOperand,
        pub dividend_high: ResOperand,
        pub divisor_low: ResOperand,
        pub divisor_high: ResOperand,
        pub quotient0: CellRef,
        pub quotient1: CellRef,
        pub divisor0: CellRef,
        pub divisor1: CellRef,
        pub extra0: CellRef,
        pub extra1: CellRef,
        pub remainder_low: CellRef,
        pub remainder_high: CellRef,
    }
    pub struct SquareRootStruct {
        pub value: ResOperand,
        pub dst: CellRef,
    }
    pub struct Uint256SquareRootStruct {
        pub value_low: ResOperand,
        pub value_high: ResOperand,
        pub sqrt0: CellRef,
        pub sqrt1: CellRef,
        pub remainder_low: CellRef,
        pub remainder_high: CellRef,
        pub sqrt_mul_2_minus_remainder_ge_u128: CellRef,
    }
    pub struct LinearSplitStruct {
        pub value: ResOperand,
        pub scalar: ResOperand,
        pub max_x: ResOperand,
        pub x: CellRef,
        pub y: CellRef,
    }
    pub struct AllocFelt252DictStruct {
        pub segment_arena_ptr: ResOperand,
    }
    pub struct Felt252DictEntryInitStruct {
        pub dict_ptr: ResOperand,
        pub key: ResOperand,
    }
    pub struct Felt252DictEntryUpdateStruct {
        pub dict_ptr: ResOperand,
        pub value: ResOperand,
    }
    pub struct GetSegmentArenaIndexStruct {
        pub dict_end_ptr: ResOperand,
        pub dict_index: CellRef,
    }
    pub struct InitSquashDataStruct {
        pub dict_accesses: ResOperand,
        pub ptr_diff: ResOperand,
        pub n_accesses: ResOperand,
        pub big_keys: CellRef,
        pub first_key: CellRef,
    }
    pub struct GetCurrentAccessIndexStruct {
        pub range_check_ptr: ResOperand,
    }
    pub struct ShouldSkipSquashLoopStruct {
        pub should_skip_loop: CellRef,
    }
    pub struct GetCurrentAccessDeltaStruct {
        pub index_delta_minus1: CellRef,
    }
    pub struct ShouldContinueSquashLoopStruct {
        pub should_continue: CellRef,
    }
    pub struct GetNextDictKeyStruct {
        pub next_key: CellRef,
    }
    pub struct AssertLeFindSmallArcsStruct {
        pub range_check_ptr: ResOperand,
        pub a: ResOperand,
        pub b: ResOperand,
    }
    pub struct AssertLeIsFirstArcExcludedStruct {
        pub skip_exclude_a_flag: CellRef,
    }
    pub struct AssertLeIsSecondArcExcludedStruct {
        pub skip_exclude_b_minus_a: CellRef,
    }
    pub struct RandomEcPointStruct {
        pub x: CellRef,
        pub y: CellRef,
    }
    pub struct FieldSqrtStruct {
        pub val: ResOperand,
        pub sqrt: CellRef,
    }
    pub struct DebugPrintStruct {
        pub start: ResOperand,
        pub end: ResOperand,
    }
    pub struct AllocConstantSizeStruct {
        pub size: ResOperand,
        pub dst: CellRef,
    }
    pub struct Uint512DivModByUint256Struct {
        pub dividend0: ResOperand,
        pub dividend1: ResOperand,
        pub dividend2: ResOperand,
        pub dividend3: ResOperand,
        pub divisor0: ResOperand,
        pub divisor1: ResOperand,
        pub quotient0: CellRef,
        pub quotient1: CellRef,
        pub quotient2: CellRef,
        pub quotient3: CellRef,
        pub remainder0: CellRef,
        pub remainder1: CellRef,
    }

    pub struct AssertAllAccessesUsedStruct {
        pub n_used_accesses: CellRef,
    }

    pub struct AssertLtAssertValidInputStruct {
        pub a: ResOperand,
        pub b: ResOperand,
    }

    pub struct Felt252DictReadStruct {
        pub dict_ptr: ResOperand,
        pub key: ResOperand,
        pub value_dst: CellRef,
    }
    pub struct Felt252DictWriteStruct {
        pub dict_ptr: ResOperand,
        pub key: ResOperand,
        pub value: ResOperand,
    }

    pub struct SetSignatureStruct {
        pub start: ResOperand,
        pub end: ResOperand,
    }
}

// Explicit implementation because of private fields.
impl StorageSerde for BigUint {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        self.to_u32_digits().serialize_into(res)?;
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Some(BigUint::from_slice(Vec::<u32>::deserialize_from(bytes)?.as_slice()))
    }
}

// TODO(yair) move enum to the macro and delete the helper struct when the macro supports named
// variables.
impl StorageSerde for CoreHint {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        CoreHintHelper::from(self.clone()).serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Some(CoreHintHelper::deserialize_from(bytes)?.into())
    }
}

// TODO(yair) move enum to the macro and delete the helper struct when the macro supports multiple
// fields in tuples.
impl StorageSerde for ResOperand {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        ResOperandHelper::from(self.clone()).serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Some(ResOperandHelper::deserialize_from(bytes)?.into())
    }
}

// TODO(yair) move enum to the macro and delete the helper struct when the macro supports named
// variables.
impl StorageSerde for DeprecatedHint {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        DeprecatedHintHelper::from(self.clone()).serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Some(DeprecatedHintHelper::deserialize_from(bytes)?.into())
    }
}

// Explicit implementation because of private fields.
impl StorageSerde for BigInt {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        let (sign, bytes) = self.to_bytes_be();
        sign.serialize_into(res)?;
        bytes.serialize_into(res)?;
        Ok(())
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        let sign = Sign::deserialize_from(bytes)?;
        let data = Vec::<u8>::deserialize_from(bytes)?;
        Some(Self::from_bytes_be(sign, data.as_slice()))
    }
}

// TODO(yair) move enum to the macro and delete the helper struct when the macro supports named
// variables.
impl StorageSerde for StarknetHint {
    fn serialize_into(&self, res: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        StarknetHintHelper::from(self.clone()).serialize_into(res)
    }

    fn deserialize_from(bytes: &mut impl std::io::Read) -> Option<Self> {
        Some(StarknetHintHelper::deserialize_from(bytes)?.into())
    }
}
