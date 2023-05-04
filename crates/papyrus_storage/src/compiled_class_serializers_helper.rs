// This file contains helper structs and enums for serializing objects that are not supported by the
// auto_storage_serde macro.
// TODO(yair): Delete this file once named variables and tuples with multiple fields are supported.

use cairo_lang_casm::hints::{CoreHint, DeprecatedHint, StarknetHint};
use cairo_lang_casm::operand::{BinOpOperand, CellRef, ResOperand};
use cairo_lang_utils::bigint::BigIntAsHex;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub enum CoreHintHelper {
    AllocSegment(AllocSegmentStruct),
    TestLessThan(TestLessThanStruct),
    TestLessThanOrEqual(TestLessThanOrEqualStruct),
    WideMul128(WideMul128Struct),
    DivMod(DivModStruct),
    Uint256DivMod(Uint256DivModStruct),
    SquareRoot(SquareRootStruct),
    Uint256SquareRoot(Uint256SquareRootStruct),
    LinearSplit(LinearSplitStruct),
    AllocFelt252Dict(AllocFelt252DictStruct),
    Felt252DictEntryInit(Felt252DictEntryInitStruct),
    Felt252DictEntryUpdate(Felt252DictEntryUpdateStruct),
    GetSegmentArenaIndex(GetSegmentArenaIndexStruct),
    InitSquashData(InitSquashDataStruct),
    GetCurrentAccessIndex(GetCurrentAccessIndexStruct),
    ShouldSkipSquashLoop(ShouldSkipSquashLoopStruct),
    GetCurrentAccessDelta(GetCurrentAccessDeltaStruct),
    ShouldContinueSquashLoop(ShouldContinueSquashLoopStruct),
    GetNextDictKey(GetNextDictKeyStruct),
    AssertLeFindSmallArcs(AssertLeFindSmallArcsStruct),
    AssertLeIsFirstArcExcluded(AssertLeIsFirstArcExcludedStruct),
    AssertLeIsSecondArcExcluded(AssertLeIsSecondArcExcludedStruct),
    RandomEcPoint(RandomEcPointStruct),
    FieldSqrt(FieldSqrtStruct),
    DebugPrint(DebugPrintStruct),
    AllocConstantSize(AllocConstantSizeStruct),
    Uint512DivModByUint256(Uint512DivModByUint256Struct),
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct AllocSegmentStruct {
    pub dst: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct TestLessThanStruct {
    pub lhs: ResOperand,
    pub rhs: ResOperand,
    pub dst: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct TestLessThanOrEqualStruct {
    pub lhs: ResOperand,
    pub rhs: ResOperand,
    pub dst: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct WideMul128Struct {
    pub lhs: ResOperand,
    pub rhs: ResOperand,
    pub high: CellRef,
    pub low: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct DivModStruct {
    pub lhs: ResOperand,
    pub rhs: ResOperand,
    pub quotient: CellRef,
    pub remainder: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
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

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct SquareRootStruct {
    pub value: ResOperand,
    pub dst: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Uint256SquareRootStruct {
    pub value_low: ResOperand,
    pub value_high: ResOperand,
    pub sqrt0: CellRef,
    pub sqrt1: CellRef,
    pub remainder_low: CellRef,
    pub remainder_high: CellRef,
    pub sqrt_mul_2_minus_remainder_ge_u128: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct LinearSplitStruct {
    pub value: ResOperand,
    pub scalar: ResOperand,
    pub max_x: ResOperand,
    pub x: CellRef,
    pub y: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct AllocFelt252DictStruct {
    pub segment_arena_ptr: ResOperand,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Felt252DictEntryInitStruct {
    pub dict_ptr: ResOperand,
    pub key: ResOperand,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Felt252DictEntryUpdateStruct {
    pub dict_ptr: ResOperand,
    pub value: ResOperand,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct GetSegmentArenaIndexStruct {
    pub dict_end_ptr: ResOperand,
    pub dict_index: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct InitSquashDataStruct {
    pub dict_accesses: ResOperand,
    pub ptr_diff: ResOperand,
    pub n_accesses: ResOperand,
    pub big_keys: CellRef,
    pub first_key: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct GetCurrentAccessIndexStruct {
    pub range_check_ptr: ResOperand,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct ShouldSkipSquashLoopStruct {
    pub should_skip_loop: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct GetCurrentAccessDeltaStruct {
    pub index_delta_minus1: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct ShouldContinueSquashLoopStruct {
    pub should_continue: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct GetNextDictKeyStruct {
    pub next_key: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct AssertLeFindSmallArcsStruct {
    pub range_check_ptr: ResOperand,
    pub a: ResOperand,
    pub b: ResOperand,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct AssertLeIsFirstArcExcludedStruct {
    pub skip_exclude_a_flag: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct AssertLeIsSecondArcExcludedStruct {
    pub skip_exclude_b_minus_a: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct RandomEcPointStruct {
    pub x: CellRef,
    pub y: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct FieldSqrtStruct {
    pub val: ResOperand,
    pub sqrt: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct DebugPrintStruct {
    pub start: ResOperand,
    pub end: ResOperand,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct AllocConstantSizeStruct {
    pub size: ResOperand,
    pub dst: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
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

impl From<CoreHint> for CoreHintHelper {
    fn from(value: CoreHint) -> Self {
        match value {
            CoreHint::AllocSegment { dst } => Self::AllocSegment(AllocSegmentStruct { dst }),
            CoreHint::TestLessThan { lhs, rhs, dst } => {
                Self::TestLessThan(TestLessThanStruct { lhs, rhs, dst })
            }
            CoreHint::TestLessThanOrEqual { lhs, rhs, dst } => {
                Self::TestLessThanOrEqual(TestLessThanOrEqualStruct { lhs, rhs, dst })
            }
            CoreHint::WideMul128 { lhs, rhs, high, low } => {
                Self::WideMul128(WideMul128Struct { lhs, rhs, high, low })
            }
            CoreHint::DivMod { lhs, rhs, quotient, remainder } => {
                Self::DivMod(DivModStruct { lhs, rhs, quotient, remainder })
            }
            CoreHint::Uint256DivMod {
                dividend_low,
                dividend_high,
                divisor_low,
                divisor_high,
                quotient0,
                quotient1,
                divisor0,
                divisor1,
                extra0,
                extra1,
                remainder_low,
                remainder_high,
            } => Self::Uint256DivMod(Uint256DivModStruct {
                dividend_low,
                dividend_high,
                divisor_low,
                divisor_high,
                quotient0,
                quotient1,
                divisor0,
                divisor1,
                extra0,
                extra1,
                remainder_low,
                remainder_high,
            }),
            CoreHint::SquareRoot { value, dst } => {
                Self::SquareRoot(SquareRootStruct { value, dst })
            }
            CoreHint::Uint256SquareRoot {
                value_low,
                value_high,
                sqrt0,
                sqrt1,
                remainder_low,
                remainder_high,
                sqrt_mul_2_minus_remainder_ge_u128,
            } => Self::Uint256SquareRoot(Uint256SquareRootStruct {
                value_low,
                value_high,
                sqrt0,
                sqrt1,
                remainder_low,
                remainder_high,
                sqrt_mul_2_minus_remainder_ge_u128,
            }),
            CoreHint::LinearSplit { value, scalar, max_x, x, y } => {
                Self::LinearSplit(LinearSplitStruct { value, scalar, max_x, x, y })
            }
            CoreHint::AllocFelt252Dict { segment_arena_ptr } => {
                Self::AllocFelt252Dict(AllocFelt252DictStruct { segment_arena_ptr })
            }
            CoreHint::Felt252DictEntryInit { dict_ptr, key } => {
                Self::Felt252DictEntryInit(Felt252DictEntryInitStruct { dict_ptr, key })
            }
            CoreHint::Felt252DictEntryUpdate { dict_ptr, value } => {
                Self::Felt252DictEntryUpdate(Felt252DictEntryUpdateStruct { dict_ptr, value })
            }
            CoreHint::GetSegmentArenaIndex { dict_end_ptr, dict_index } => {
                Self::GetSegmentArenaIndex(GetSegmentArenaIndexStruct { dict_end_ptr, dict_index })
            }
            CoreHint::InitSquashData {
                dict_accesses,
                ptr_diff,
                n_accesses,
                big_keys,
                first_key,
            } => Self::InitSquashData(InitSquashDataStruct {
                dict_accesses,
                ptr_diff,
                n_accesses,
                big_keys,
                first_key,
            }),
            CoreHint::GetCurrentAccessIndex { range_check_ptr } => {
                Self::GetCurrentAccessIndex(GetCurrentAccessIndexStruct { range_check_ptr })
            }
            CoreHint::ShouldSkipSquashLoop { should_skip_loop } => {
                Self::ShouldSkipSquashLoop(ShouldSkipSquashLoopStruct { should_skip_loop })
            }
            CoreHint::GetCurrentAccessDelta { index_delta_minus1 } => {
                Self::GetCurrentAccessDelta(GetCurrentAccessDeltaStruct { index_delta_minus1 })
            }
            CoreHint::ShouldContinueSquashLoop { should_continue } => {
                Self::ShouldContinueSquashLoop(ShouldContinueSquashLoopStruct { should_continue })
            }
            CoreHint::GetNextDictKey { next_key } => {
                Self::GetNextDictKey(GetNextDictKeyStruct { next_key })
            }
            CoreHint::AssertLeFindSmallArcs { range_check_ptr, a, b } => {
                Self::AssertLeFindSmallArcs(AssertLeFindSmallArcsStruct { range_check_ptr, a, b })
            }
            CoreHint::AssertLeIsFirstArcExcluded { skip_exclude_a_flag } => {
                Self::AssertLeIsFirstArcExcluded(AssertLeIsFirstArcExcludedStruct {
                    skip_exclude_a_flag,
                })
            }
            CoreHint::AssertLeIsSecondArcExcluded { skip_exclude_b_minus_a } => {
                Self::AssertLeIsSecondArcExcluded(AssertLeIsSecondArcExcludedStruct {
                    skip_exclude_b_minus_a,
                })
            }
            CoreHint::RandomEcPoint { x, y } => Self::RandomEcPoint(RandomEcPointStruct { x, y }),
            CoreHint::FieldSqrt { val, sqrt } => Self::FieldSqrt(FieldSqrtStruct { val, sqrt }),
            CoreHint::DebugPrint { start, end } => {
                Self::DebugPrint(DebugPrintStruct { start, end })
            }
            CoreHint::AllocConstantSize { size, dst } => {
                Self::AllocConstantSize(AllocConstantSizeStruct { size, dst })
            }
            CoreHint::Uint512DivModByUint256 {
                dividend0,
                dividend1,
                dividend2,
                dividend3,
                divisor0,
                divisor1,
                quotient0,
                quotient1,
                quotient2,
                quotient3,
                remainder0,
                remainder1,
            } => Self::Uint512DivModByUint256(Uint512DivModByUint256Struct {
                dividend0,
                dividend1,
                dividend2,
                dividend3,
                divisor0,
                divisor1,
                quotient0,
                quotient1,
                quotient2,
                quotient3,
                remainder0,
                remainder1,
            }),
        }
    }
}

impl From<CoreHintHelper> for CoreHint {
    fn from(value: CoreHintHelper) -> Self {
        match value {
            CoreHintHelper::AllocSegment(AllocSegmentStruct { dst }) => Self::AllocSegment { dst },
            CoreHintHelper::TestLessThan(TestLessThanStruct { lhs, rhs, dst }) => {
                Self::TestLessThan { lhs, rhs, dst }
            }
            CoreHintHelper::TestLessThanOrEqual(TestLessThanOrEqualStruct { lhs, rhs, dst }) => {
                Self::TestLessThanOrEqual { lhs, rhs, dst }
            }
            CoreHintHelper::WideMul128(WideMul128Struct { lhs, rhs, high, low }) => {
                Self::WideMul128 { lhs, rhs, high, low }
            }
            CoreHintHelper::DivMod(DivModStruct { lhs, rhs, quotient, remainder }) => {
                Self::DivMod { lhs, rhs, quotient, remainder }
            }
            CoreHintHelper::Uint256DivMod(Uint256DivModStruct {
                dividend_low,
                dividend_high,
                divisor_low,
                divisor_high,
                quotient0,
                quotient1,
                divisor0,
                divisor1,
                extra0,
                extra1,
                remainder_low,
                remainder_high,
            }) => Self::Uint256DivMod {
                dividend_low,
                dividend_high,
                divisor_low,
                divisor_high,
                quotient0,
                quotient1,
                divisor0,
                divisor1,
                extra0,
                extra1,
                remainder_low,
                remainder_high,
            },
            CoreHintHelper::SquareRoot(SquareRootStruct { value, dst }) => {
                Self::SquareRoot { value, dst }
            }
            CoreHintHelper::Uint256SquareRoot(Uint256SquareRootStruct {
                value_low,
                value_high,
                sqrt0,
                sqrt1,
                remainder_low,
                remainder_high,
                sqrt_mul_2_minus_remainder_ge_u128,
            }) => Self::Uint256SquareRoot {
                value_low,
                value_high,
                sqrt0,
                sqrt1,
                remainder_low,
                remainder_high,
                sqrt_mul_2_minus_remainder_ge_u128,
            },
            CoreHintHelper::LinearSplit(LinearSplitStruct { value, scalar, max_x, x, y }) => {
                Self::LinearSplit { value, scalar, max_x, x, y }
            }
            CoreHintHelper::AllocFelt252Dict(AllocFelt252DictStruct { segment_arena_ptr }) => {
                Self::AllocFelt252Dict { segment_arena_ptr }
            }
            CoreHintHelper::Felt252DictEntryInit(Felt252DictEntryInitStruct { dict_ptr, key }) => {
                Self::Felt252DictEntryInit { dict_ptr, key }
            }
            CoreHintHelper::Felt252DictEntryUpdate(Felt252DictEntryUpdateStruct {
                dict_ptr,
                value,
            }) => Self::Felt252DictEntryUpdate { dict_ptr, value },
            CoreHintHelper::GetSegmentArenaIndex(GetSegmentArenaIndexStruct {
                dict_end_ptr,
                dict_index,
            }) => Self::GetSegmentArenaIndex { dict_end_ptr, dict_index },
            CoreHintHelper::InitSquashData(InitSquashDataStruct {
                dict_accesses,
                ptr_diff,
                n_accesses,
                big_keys,
                first_key,
            }) => Self::InitSquashData { dict_accesses, ptr_diff, n_accesses, big_keys, first_key },
            CoreHintHelper::GetCurrentAccessIndex(GetCurrentAccessIndexStruct {
                range_check_ptr,
            }) => Self::GetCurrentAccessIndex { range_check_ptr },
            CoreHintHelper::ShouldSkipSquashLoop(ShouldSkipSquashLoopStruct {
                should_skip_loop,
            }) => Self::ShouldSkipSquashLoop { should_skip_loop },
            CoreHintHelper::GetCurrentAccessDelta(GetCurrentAccessDeltaStruct {
                index_delta_minus1,
            }) => Self::GetCurrentAccessDelta { index_delta_minus1 },
            CoreHintHelper::ShouldContinueSquashLoop(ShouldContinueSquashLoopStruct {
                should_continue,
            }) => Self::ShouldContinueSquashLoop { should_continue },
            CoreHintHelper::GetNextDictKey(GetNextDictKeyStruct { next_key }) => {
                Self::GetNextDictKey { next_key }
            }
            CoreHintHelper::AssertLeFindSmallArcs(AssertLeFindSmallArcsStruct {
                range_check_ptr,
                a,
                b,
            }) => Self::AssertLeFindSmallArcs { range_check_ptr, a, b },
            CoreHintHelper::AssertLeIsFirstArcExcluded(AssertLeIsFirstArcExcludedStruct {
                skip_exclude_a_flag,
            }) => Self::AssertLeIsFirstArcExcluded { skip_exclude_a_flag },
            CoreHintHelper::AssertLeIsSecondArcExcluded(AssertLeIsSecondArcExcludedStruct {
                skip_exclude_b_minus_a,
            }) => Self::AssertLeIsSecondArcExcluded { skip_exclude_b_minus_a },
            CoreHintHelper::RandomEcPoint(RandomEcPointStruct { x, y }) => {
                Self::RandomEcPoint { x, y }
            }
            CoreHintHelper::FieldSqrt(FieldSqrtStruct { val, sqrt }) => {
                Self::FieldSqrt { val, sqrt }
            }
            CoreHintHelper::DebugPrint(DebugPrintStruct { start, end }) => {
                Self::DebugPrint { start, end }
            }
            CoreHintHelper::AllocConstantSize(AllocConstantSizeStruct { size, dst }) => {
                Self::AllocConstantSize { size, dst }
            }
            CoreHintHelper::Uint512DivModByUint256(Uint512DivModByUint256Struct {
                dividend0,
                dividend1,
                dividend2,
                dividend3,
                divisor0,
                divisor1,
                quotient0,
                quotient1,
                quotient2,
                quotient3,
                remainder0,
                remainder1,
            }) => Self::Uint512DivModByUint256 {
                dividend0,
                dividend1,
                dividend2,
                dividend3,
                divisor0,
                divisor1,
                quotient0,
                quotient1,
                quotient2,
                quotient3,
                remainder0,
                remainder1,
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub enum ResOperandHelper {
    Deref(CellRef),
    DoubleDeref(DoubleDerefStruct),
    Immediate(BigIntAsHex),
    BinOp(BinOpOperand),
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct DoubleDerefStruct(pub CellRef, pub i16);

impl From<ResOperand> for ResOperandHelper {
    fn from(value: ResOperand) -> Self {
        match value {
            ResOperand::Deref(v) => Self::Deref(v),
            ResOperand::DoubleDeref(v0, v1) => Self::DoubleDeref(DoubleDerefStruct(v0, v1)),
            ResOperand::Immediate(v) => Self::Immediate(v),
            ResOperand::BinOp(v) => Self::BinOp(v),
        }
    }
}

impl From<ResOperandHelper> for ResOperand {
    fn from(value: ResOperandHelper) -> Self {
        match value {
            ResOperandHelper::Deref(v) => Self::Deref(v),
            ResOperandHelper::DoubleDeref(v) => Self::DoubleDeref(v.0, v.1),
            ResOperandHelper::Immediate(v) => Self::Immediate(v),
            ResOperandHelper::BinOp(v) => Self::BinOp(v),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub enum DeprecatedHintHelper {
    AssertCurrentAccessIndicesIsEmpty,
    AssertAllAccessesUsed(AssertAllAccessesUsedStruct),
    AssertAllKeysUsed,
    AssertLeAssertThirdArcExcluded,
    AssertLtAssertValidInput(AssertLtAssertValidInputStruct),
    Felt252DictRead(Felt252DictReadStruct),
    Felt252DictWrite(Felt252DictWriteStruct),
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct AssertAllAccessesUsedStruct {
    pub n_used_accesses: CellRef,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct AssertLtAssertValidInputStruct {
    pub a: ResOperand,
    pub b: ResOperand,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Felt252DictReadStruct {
    pub dict_ptr: ResOperand,
    pub key: ResOperand,
    pub value_dst: CellRef,
}
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Felt252DictWriteStruct {
    pub dict_ptr: ResOperand,
    pub key: ResOperand,
    pub value: ResOperand,
}

impl From<DeprecatedHint> for DeprecatedHintHelper {
    fn from(value: DeprecatedHint) -> Self {
        match value {
            DeprecatedHint::AssertCurrentAccessIndicesIsEmpty => {
                Self::AssertCurrentAccessIndicesIsEmpty
            }
            DeprecatedHint::AssertAllAccessesUsed { n_used_accesses } => {
                Self::AssertAllAccessesUsed(AssertAllAccessesUsedStruct { n_used_accesses })
            }
            DeprecatedHint::AssertAllKeysUsed => Self::AssertAllKeysUsed,
            DeprecatedHint::AssertLeAssertThirdArcExcluded => Self::AssertLeAssertThirdArcExcluded,
            DeprecatedHint::AssertLtAssertValidInput { a, b } => {
                Self::AssertLtAssertValidInput(AssertLtAssertValidInputStruct { a, b })
            }
            DeprecatedHint::Felt252DictRead { dict_ptr, key, value_dst } => {
                Self::Felt252DictRead(Felt252DictReadStruct { dict_ptr, key, value_dst })
            }
            DeprecatedHint::Felt252DictWrite { dict_ptr, key, value } => {
                Self::Felt252DictWrite(Felt252DictWriteStruct { dict_ptr, key, value })
            }
        }
    }
}

impl From<DeprecatedHintHelper> for DeprecatedHint {
    fn from(value: DeprecatedHintHelper) -> Self {
        match value {
            DeprecatedHintHelper::AssertCurrentAccessIndicesIsEmpty => {
                Self::AssertCurrentAccessIndicesIsEmpty
            }
            DeprecatedHintHelper::AssertAllAccessesUsed(AssertAllAccessesUsedStruct {
                n_used_accesses,
            }) => Self::AssertAllAccessesUsed { n_used_accesses },
            DeprecatedHintHelper::AssertAllKeysUsed => Self::AssertAllKeysUsed,
            DeprecatedHintHelper::AssertLeAssertThirdArcExcluded => {
                Self::AssertLeAssertThirdArcExcluded
            }
            DeprecatedHintHelper::AssertLtAssertValidInput(AssertLtAssertValidInputStruct {
                a,
                b,
            }) => Self::AssertLtAssertValidInput { a, b },
            DeprecatedHintHelper::Felt252DictRead(Felt252DictReadStruct {
                dict_ptr,
                key,
                value_dst,
            }) => Self::Felt252DictRead { dict_ptr, key, value_dst },
            DeprecatedHintHelper::Felt252DictWrite(Felt252DictWriteStruct {
                dict_ptr,
                key,
                value,
            }) => Self::Felt252DictWrite { dict_ptr, key, value },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub enum StarknetHintHelper {
    SystemCall(ResOperand),
    SetBlockNumber(ResOperand),
    SetBlockTimestamp(ResOperand),
    SetCallerAddress(ResOperand),
    SetContractAddress(ResOperand),
    SetSequencerAddress(ResOperand),
    SetVersion(ResOperand),
    SetAccountContractAddress(ResOperand),
    SetMaxFee(ResOperand),
    SetTransactionHash(ResOperand),
    SetChainId(ResOperand),
    SetNonce(ResOperand),
    SetSignature(SetSignatureStruct),
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct SetSignatureStruct {
    pub start: ResOperand,
    pub end: ResOperand,
}

impl From<StarknetHint> for StarknetHintHelper {
    fn from(value: StarknetHint) -> Self {
        match value {
            StarknetHint::SystemCall { system } => Self::SystemCall(system),
            StarknetHint::SetBlockNumber { value } => Self::SetBlockNumber(value),
            StarknetHint::SetBlockTimestamp { value } => Self::SetBlockTimestamp(value),
            StarknetHint::SetCallerAddress { value } => Self::SetCallerAddress(value),
            StarknetHint::SetContractAddress { value } => Self::SetContractAddress(value),
            StarknetHint::SetSequencerAddress { value } => Self::SetSequencerAddress(value),
            StarknetHint::SetVersion { value } => Self::SetVersion(value),
            StarknetHint::SetAccountContractAddress { value } => {
                Self::SetAccountContractAddress(value)
            }
            StarknetHint::SetMaxFee { value } => Self::SetMaxFee(value),
            StarknetHint::SetTransactionHash { value } => Self::SetTransactionHash(value),
            StarknetHint::SetChainId { value } => Self::SetChainId(value),
            StarknetHint::SetNonce { value } => Self::SetNonce(value),
            StarknetHint::SetSignature { start, end } => {
                Self::SetSignature(SetSignatureStruct { start, end })
            }
        }
    }
}

impl From<StarknetHintHelper> for StarknetHint {
    fn from(value: StarknetHintHelper) -> Self {
        match value {
            StarknetHintHelper::SystemCall(system) => Self::SystemCall { system },
            StarknetHintHelper::SetBlockNumber(value) => Self::SetBlockNumber { value },
            StarknetHintHelper::SetBlockTimestamp(value) => Self::SetBlockTimestamp { value },
            StarknetHintHelper::SetCallerAddress(value) => Self::SetCallerAddress { value },
            StarknetHintHelper::SetContractAddress(value) => Self::SetContractAddress { value },
            StarknetHintHelper::SetSequencerAddress(value) => Self::SetSequencerAddress { value },
            StarknetHintHelper::SetVersion(value) => Self::SetVersion { value },
            StarknetHintHelper::SetAccountContractAddress(value) => {
                Self::SetAccountContractAddress { value }
            }
            StarknetHintHelper::SetMaxFee(value) => Self::SetMaxFee { value },
            StarknetHintHelper::SetTransactionHash(value) => Self::SetTransactionHash { value },
            StarknetHintHelper::SetChainId(value) => Self::SetChainId { value },
            StarknetHintHelper::SetNonce(value) => Self::SetNonce { value },
            StarknetHintHelper::SetSignature(SetSignatureStruct { start, end }) => {
                Self::SetSignature { start, end }
            }
        }
    }
}
