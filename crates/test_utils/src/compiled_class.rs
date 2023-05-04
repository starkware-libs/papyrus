use cairo_lang_casm::hints::{CoreHint, CoreHintBase, DeprecatedHint, Hint, StarknetHint};
use cairo_lang_casm::operand::{
    BinOpOperand, CellRef, DerefOrImmediate, Operation, Register, ResOperand,
};
use cairo_lang_starknet::casm_contract_class::{
    CasmContractClass, CasmContractEntryPoint, CasmContractEntryPoints,
};
use cairo_lang_utils::bigint::{BigIntAsHex, BigUintAsHex};
use num_bigint::{BigInt, BigUint, Sign};
use rand_chacha::ChaCha8Rng;

use crate::GetTestInstance;

pub fn get_test_casm() -> CasmContractClass {
    let casm_json = r#"{
    "entry_points_by_type": {
        "EXTERNAL": [
            {
                "offset": 787,
                "builtins": [
                    "pedersen",
                    "range_check"
                ],
                "selector": "0x11dd528db174d6312644720bceeb9307ba53f6e2937246ac73d5fb30603016"
            }
        ],
        "L1_HANDLER": [],
        "CONSTRUCTOR": [
            {
                "offset": 4305,
                "builtins": [
                    "range_check"
                ],
                "selector": "0x28ffe4ff0f226a9107253e17a904099aa4f63a02a5621de0576e5aa71bc5194"
            }
        ]
    },
    "bytecode": [
        "0x40780017fff7fff",
        "0x2",
        "0x496e70757420746f6f2073686f727420666f7220617267756d656e7473"
    ],
    "prime": "0x800000000000011000000000000000000000000000000000000000000000001",
    "pythonic_hints": [
        [
            2,
            [
                "memory[ap + 0] = 0 <= memory[fp + -6]"
            ]
        ]
    ],
    "hints": [
        [
            2,
            [
                {
                    "TestLessThanOrEqual": {
                        "lhs": {
                            "Immediate": "0x0"
                        },
                        "rhs": {
                            "Deref": {
                                "register": "FP",
                                "offset": -6
                            }
                        },
                        "dst": {
                            "register": "AP",
                            "offset": 0
                        }
                    }
                }
            ]
        ]
    ],
    "compiler_version": "1.0.0"
}"#;
    serde_json::from_str::<CasmContractClass>(casm_json).unwrap()
}

// TODO(yair): Create a random instances for all of the types (using the macro?).
impl GetTestInstance for CasmContractClass {
    fn get_test_instance(_rng: &mut ChaCha8Rng) -> Self {
        get_test_casm()
    }
}

impl GetTestInstance for Hint {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        let casm = CasmContractClass::get_test_instance(rng);
        casm.hints.first().unwrap().1.first().unwrap().clone()
    }
}

impl GetTestInstance for CasmContractEntryPoints {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        let casm = CasmContractClass::get_test_instance(rng);
        casm.entry_points_by_type
    }
}

impl GetTestInstance for CasmContractEntryPoint {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        CasmContractEntryPoints::get_test_instance(rng).external.first().unwrap().clone()
    }
}

impl GetTestInstance for DeprecatedHint {
    fn get_test_instance(_rng: &mut ChaCha8Rng) -> Self {
        Self::AssertCurrentAccessIndicesIsEmpty
    }
}

impl GetTestInstance for StarknetHint {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Self::SystemCall { system: ResOperand::get_test_instance(rng) }
    }
}

impl GetTestInstance for CoreHint {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Self::AllocSegment { dst: CellRef::get_test_instance(rng) }
    }
}

impl GetTestInstance for CoreHintBase {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Self::Core(CoreHint::get_test_instance(rng))
    }
}

impl GetTestInstance for ResOperand {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Self::Deref(CellRef::get_test_instance(rng))
    }
}

impl GetTestInstance for CellRef {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Self { register: Register::get_test_instance(rng), offset: i16::get_test_instance(rng) }
    }
}

impl GetTestInstance for Register {
    fn get_test_instance(_rng: &mut ChaCha8Rng) -> Self {
        Self::AP
    }
}

impl GetTestInstance for BigUint {
    fn get_test_instance(_rng: &mut ChaCha8Rng) -> Self {
        Self::default()
    }
}

impl GetTestInstance for BigInt {
    fn get_test_instance(_rng: &mut ChaCha8Rng) -> Self {
        Self::default()
    }
}

impl GetTestInstance for BigUintAsHex {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Self { value: BigUint::get_test_instance(rng) }
    }
}

impl GetTestInstance for BigIntAsHex {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Self { value: BigInt::get_test_instance(rng) }
    }
}

impl GetTestInstance for Sign {
    fn get_test_instance(_rng: &mut ChaCha8Rng) -> Self {
        Self::NoSign
    }
}

impl GetTestInstance for BinOpOperand {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Self {
            op: Operation::get_test_instance(rng),
            a: CellRef::get_test_instance(rng),
            b: DerefOrImmediate::get_test_instance(rng),
        }
    }
}

impl GetTestInstance for Operation {
    fn get_test_instance(_rng: &mut ChaCha8Rng) -> Self {
        Self::Add
    }
}

impl GetTestInstance for DerefOrImmediate {
    fn get_test_instance(rng: &mut ChaCha8Rng) -> Self {
        Self::Deref(CellRef::get_test_instance(rng))
    }
}
