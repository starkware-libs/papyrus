use std::fmt::Debug;

use cairo_lang_casm::operand::{BinOpOperand, CellRef, ResOperand};
use cairo_lang_utils::bigint::BigIntAsHex;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::hash::StarkHash;
use starknet_api::state::StorageKey;
use test_utils::{auto_impl_get_test_instance, get_number_of_variants, get_rng, GetTestInstance};

use crate::compiled_class_serializers_helper::*;
use crate::db::serialization::StorageSerde;

pub trait StorageSerdeTest: StorageSerde {
    fn storage_serde_test();
}

// Implements the [`storage_serde_test`] function for every type that
// implements the [`StorageSerde`] and [`GetTestInstance`] traits.
impl<T: StorageSerde + GetTestInstance + Eq + Debug> StorageSerdeTest for T {
    fn storage_serde_test() {
        let mut rng = get_rng(None);
        let item = T::get_test_instance(&mut rng);
        let mut serialized: Vec<u8> = Vec::new();
        item.serialize_into(&mut serialized).unwrap();
        let bytes = serialized.into_boxed_slice();
        let deserialized = T::deserialize_from(&mut bytes.as_ref());
        assert_eq!(item, deserialized.unwrap());
    }
}

// Tests all types that implement the [`StorageSerde`] trait
// via the [`auto_storage_serde`] macro.
macro_rules! create_storage_serde_test {
    ($name:ident) => {
        paste::paste! {
            #[test]
            fn [<"storage_serde_test" _$name:snake>]() {
                $name::storage_serde_test()
            }
        }
    };
}
pub(crate) use create_storage_serde_test;

////////////////////////////////////////////////////////////////////////
// Implements the [`GetTestInstance`] trait for types not supported
// by the macro [`impl_get_test_instance`] and calls the [`create_test`]
// macro to create the tests for them.
////////////////////////////////////////////////////////////////////////
create_storage_serde_test!(bool);
create_storage_serde_test!(ContractAddress);
create_storage_serde_test!(StarkHash);
create_storage_serde_test!(StorageKey);
create_storage_serde_test!(u8);
create_storage_serde_test!(usize);

#[test]
fn block_number_endianness() {
    let bn_255 = BlockNumber(255);
    let mut serialized: Vec<u8> = Vec::new();
    bn_255.serialize_into(&mut serialized).unwrap();
    let bytes_255 = serialized.into_boxed_slice();
    let deserialized = BlockNumber::deserialize_from(&mut bytes_255.as_ref());
    assert_eq!(bn_255, deserialized.unwrap());

    let bn_256 = BlockNumber(256);
    let mut serialized: Vec<u8> = Vec::new();
    bn_256.serialize_into(&mut serialized).unwrap();
    let bytes_256 = serialized.into_boxed_slice();
    let deserialized = BlockNumber::deserialize_from(&mut bytes_256.as_ref());
    assert_eq!(bn_256, deserialized.unwrap());

    assert!(bytes_255 < bytes_256);
}

// These are stucts defined in this crate, so we can't import them into test_utils
// because it will cause a circular dependency.
// Once the macro will support all kind of enums variants, these structs will be deleted.
auto_impl_get_test_instance! {
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
