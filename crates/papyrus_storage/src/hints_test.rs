use cairo_lang_casm::hints::{CoreHint, CoreHintBase, DeprecatedHint, Hint, StarknetHint};
use cairo_lang_casm::operand::Operation::{self, Add};
use cairo_lang_casm::operand::{BinOpOperand, CellRef, DerefOrImmediate, Register, ResOperand};
use cairo_lang_utils::bigint::BigIntAsHex;
use test_utils::read_json_file;

#[test]
fn hints_serde_regression_test() {
    // The hints were generated using 'generate_hints_for_regression_test'. In case there's a change
    // in the hints' serialization, use this function to re-generate them.
    let serialized_hints = read_json_file("hints.json");
    let serialized_hints_vec = serialized_hints.as_array().unwrap();
    for serialized_hint in serialized_hints_vec {
        // Make sure we are able to deserialize each hint as it is currently stored.
        let deserialized_hint: Hint = serde_json::from_value(serialized_hint.clone()).unwrap();
        // Make sure the serialization of the hint wasn't changed.
        let reserialized_hint = serde_json::to_value(&deserialized_hint).unwrap();
        assert_eq!(
            serialized_hint.clone(),
            reserialized_hint,
            "The serialization of hint changed, need to write a storage migration and update the \
             serialization of the hints for this test.\nHint: {deserialized_hint:#?}",
        );
    }
}

#[allow(dead_code)]
fn generate_hints_for_regression_test() -> Vec<Hint> {
    fn next<T: Clone>(variants: &Vec<T>, cur: &mut usize) -> T {
        if *cur == variants.len() {
            return variants[0].clone();
        } else {
            *cur += 1;
        }
        variants[*cur - 1].clone()
    }

    let cell_ref_vec = vec![
        CellRef { register: Register::AP, offset: 0 },
        CellRef { register: Register::FP, offset: 0 },
    ];
    let mut cell_ref_i = 0;

    let deref_or_immediate_vec = vec![
        DerefOrImmediate::Deref(next(&cell_ref_vec, &mut cell_ref_i)),
        DerefOrImmediate::Immediate(BigIntAsHex::default()),
    ];
    let mut deref_or_immediate_i = 0;

    let bin_op_operand_vec = vec![
        BinOpOperand {
            op: Add,
            a: next(&cell_ref_vec, &mut cell_ref_i),
            b: next(&deref_or_immediate_vec, &mut deref_or_immediate_i),
        },
        BinOpOperand {
            op: Operation::Mul,
            a: next(&cell_ref_vec, &mut cell_ref_i),
            b: next(&deref_or_immediate_vec, &mut deref_or_immediate_i),
        },
    ];
    let mut bin_op_operand_i = 0;

    let res_operand_vec = vec![
        ResOperand::Deref(next(&cell_ref_vec, &mut cell_ref_i)),
        ResOperand::DoubleDeref(next(&cell_ref_vec, &mut cell_ref_i), 0),
        ResOperand::Immediate(BigIntAsHex::default()),
        ResOperand::BinOp(next(&bin_op_operand_vec, &mut bin_op_operand_i)),
        // BinOp not spanned in other places, so I create another instance here.
        ResOperand::BinOp(next(&bin_op_operand_vec, &mut bin_op_operand_i)),
    ];
    let mut res_operand_i = 0;

    let hints: Vec<Hint> = vec![
        // CoreHint variants.
        Hint::Core(CoreHintBase::Core(CoreHint::AllocSegment {
            dst: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::TestLessThan {
            lhs: next(&res_operand_vec, &mut res_operand_i),
            rhs: next(&res_operand_vec, &mut res_operand_i),
            dst: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::TestLessThanOrEqual {
            lhs: next(&res_operand_vec, &mut res_operand_i),
            rhs: next(&res_operand_vec, &mut res_operand_i),
            dst: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::WideMul128 {
            lhs: next(&res_operand_vec, &mut res_operand_i),
            rhs: next(&res_operand_vec, &mut res_operand_i),
            high: next(&cell_ref_vec, &mut cell_ref_i),
            low: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::DivMod {
            lhs: next(&res_operand_vec, &mut res_operand_i),
            rhs: next(&res_operand_vec, &mut res_operand_i),
            quotient: next(&cell_ref_vec, &mut cell_ref_i),
            remainder: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::Uint256DivMod {
            dividend_low: next(&res_operand_vec, &mut res_operand_i),
            dividend_high: next(&res_operand_vec, &mut res_operand_i),
            divisor_low: next(&res_operand_vec, &mut res_operand_i),
            divisor_high: next(&res_operand_vec, &mut res_operand_i),
            quotient0: next(&cell_ref_vec, &mut cell_ref_i),
            quotient1: next(&cell_ref_vec, &mut cell_ref_i),
            divisor0: next(&cell_ref_vec, &mut cell_ref_i),
            divisor1: next(&cell_ref_vec, &mut cell_ref_i),
            extra0: next(&cell_ref_vec, &mut cell_ref_i),
            extra1: next(&cell_ref_vec, &mut cell_ref_i),
            remainder_low: next(&cell_ref_vec, &mut cell_ref_i),
            remainder_high: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::Uint512DivModByUint256 {
            dividend0: next(&res_operand_vec, &mut res_operand_i),
            dividend1: next(&res_operand_vec, &mut res_operand_i),
            dividend2: next(&res_operand_vec, &mut res_operand_i),
            dividend3: next(&res_operand_vec, &mut res_operand_i),
            divisor0: next(&res_operand_vec, &mut res_operand_i),
            divisor1: next(&res_operand_vec, &mut res_operand_i),
            quotient0: next(&cell_ref_vec, &mut cell_ref_i),
            quotient1: next(&cell_ref_vec, &mut cell_ref_i),
            quotient2: next(&cell_ref_vec, &mut cell_ref_i),
            quotient3: next(&cell_ref_vec, &mut cell_ref_i),
            remainder0: next(&cell_ref_vec, &mut cell_ref_i),
            remainder1: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::SquareRoot {
            value: next(&res_operand_vec, &mut res_operand_i),
            dst: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::Uint256SquareRoot {
            value_low: next(&res_operand_vec, &mut res_operand_i),
            value_high: next(&res_operand_vec, &mut res_operand_i),
            sqrt0: next(&cell_ref_vec, &mut cell_ref_i),
            sqrt1: next(&cell_ref_vec, &mut cell_ref_i),
            remainder_low: next(&cell_ref_vec, &mut cell_ref_i),
            remainder_high: next(&cell_ref_vec, &mut cell_ref_i),
            sqrt_mul_2_minus_remainder_ge_u128: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::LinearSplit {
            value: next(&res_operand_vec, &mut res_operand_i),
            scalar: next(&res_operand_vec, &mut res_operand_i),
            max_x: next(&res_operand_vec, &mut res_operand_i),
            x: next(&cell_ref_vec, &mut cell_ref_i),
            y: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::AllocFelt252Dict {
            segment_arena_ptr: next(&res_operand_vec, &mut res_operand_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::Felt252DictEntryInit {
            dict_ptr: next(&res_operand_vec, &mut res_operand_i),
            key: next(&res_operand_vec, &mut res_operand_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::Felt252DictEntryUpdate {
            dict_ptr: next(&res_operand_vec, &mut res_operand_i),
            value: next(&res_operand_vec, &mut res_operand_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::GetSegmentArenaIndex {
            dict_end_ptr: next(&res_operand_vec, &mut res_operand_i),
            dict_index: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::InitSquashData {
            dict_accesses: next(&res_operand_vec, &mut res_operand_i),
            ptr_diff: next(&res_operand_vec, &mut res_operand_i),
            n_accesses: next(&res_operand_vec, &mut res_operand_i),
            big_keys: next(&cell_ref_vec, &mut cell_ref_i),
            first_key: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::GetCurrentAccessIndex {
            range_check_ptr: next(&res_operand_vec, &mut res_operand_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::ShouldSkipSquashLoop {
            should_skip_loop: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::GetCurrentAccessDelta {
            index_delta_minus1: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::ShouldContinueSquashLoop {
            should_continue: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::GetNextDictKey {
            next_key: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::AssertLeFindSmallArcs {
            range_check_ptr: next(&res_operand_vec, &mut res_operand_i),
            a: next(&res_operand_vec, &mut res_operand_i),
            b: next(&res_operand_vec, &mut res_operand_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::AssertLeIsFirstArcExcluded {
            skip_exclude_a_flag: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::AssertLeIsSecondArcExcluded {
            skip_exclude_b_minus_a: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::RandomEcPoint {
            x: next(&cell_ref_vec, &mut cell_ref_i),
            y: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::FieldSqrt {
            val: next(&res_operand_vec, &mut res_operand_i),
            sqrt: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::DebugPrint {
            start: next(&res_operand_vec, &mut res_operand_i),
            end: next(&res_operand_vec, &mut res_operand_i),
        })),
        Hint::Core(CoreHintBase::Core(CoreHint::AllocConstantSize {
            size: next(&res_operand_vec, &mut res_operand_i),
            dst: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        // Deprecated hints
        Hint::Core(CoreHintBase::Deprecated(DeprecatedHint::AssertCurrentAccessIndicesIsEmpty)),
        Hint::Core(CoreHintBase::Deprecated(DeprecatedHint::AssertAllAccessesUsed {
            n_used_accesses: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Deprecated(DeprecatedHint::AssertAllKeysUsed)),
        Hint::Core(CoreHintBase::Deprecated(DeprecatedHint::AssertLeAssertThirdArcExcluded)),
        Hint::Core(CoreHintBase::Deprecated(DeprecatedHint::AssertLtAssertValidInput {
            a: next(&res_operand_vec, &mut res_operand_i),
            b: next(&res_operand_vec, &mut res_operand_i),
        })),
        Hint::Core(CoreHintBase::Deprecated(DeprecatedHint::Felt252DictRead {
            dict_ptr: next(&res_operand_vec, &mut res_operand_i),
            key: next(&res_operand_vec, &mut res_operand_i),
            value_dst: next(&cell_ref_vec, &mut cell_ref_i),
        })),
        Hint::Core(CoreHintBase::Deprecated(DeprecatedHint::Felt252DictWrite {
            dict_ptr: next(&res_operand_vec, &mut res_operand_i),
            key: next(&res_operand_vec, &mut res_operand_i),
            value: next(&res_operand_vec, &mut res_operand_i),
        })),
        // Starknet hints
        Hint::Starknet(StarknetHint::SystemCall {
            system: next(&res_operand_vec, &mut res_operand_i),
        }),
        Hint::Starknet(StarknetHint::SetBlockNumber {
            value: next(&res_operand_vec, &mut res_operand_i),
        }),
        Hint::Starknet(StarknetHint::SetBlockTimestamp {
            value: next(&res_operand_vec, &mut res_operand_i),
        }),
        Hint::Starknet(StarknetHint::SetCallerAddress {
            value: next(&res_operand_vec, &mut res_operand_i),
        }),
        Hint::Starknet(StarknetHint::SetContractAddress {
            value: next(&res_operand_vec, &mut res_operand_i),
        }),
        Hint::Starknet(StarknetHint::SetSequencerAddress {
            value: next(&res_operand_vec, &mut res_operand_i),
        }),
        Hint::Starknet(StarknetHint::SetVersion {
            value: next(&res_operand_vec, &mut res_operand_i),
        }),
        Hint::Starknet(StarknetHint::SetAccountContractAddress {
            value: next(&res_operand_vec, &mut res_operand_i),
        }),
        Hint::Starknet(StarknetHint::SetMaxFee {
            value: next(&res_operand_vec, &mut res_operand_i),
        }),
        Hint::Starknet(StarknetHint::SetTransactionHash {
            value: next(&res_operand_vec, &mut res_operand_i),
        }),
        Hint::Starknet(StarknetHint::SetChainId {
            value: next(&res_operand_vec, &mut res_operand_i),
        }),
        Hint::Starknet(StarknetHint::SetNonce {
            value: next(&res_operand_vec, &mut res_operand_i),
        }),
        Hint::Starknet(StarknetHint::SetSignature {
            start: next(&res_operand_vec, &mut res_operand_i),
            end: next(&res_operand_vec, &mut res_operand_i),
        }),
    ];

    assert_eq!(cell_ref_vec.len(), cell_ref_i);
    assert_eq!(deref_or_immediate_vec.len(), deref_or_immediate_i);
    assert_eq!(bin_op_operand_vec.len(), bin_op_operand_i);
    assert_eq!(res_operand_vec.len(), res_operand_i);

    hints
}
