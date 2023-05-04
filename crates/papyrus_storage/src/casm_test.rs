use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use starknet_api::core::ClassHash;

use crate::casm::{CasmStorageReader, CasmStorageWriter};
use crate::test_utils::get_test_storage;

#[test]
fn append_casm() {
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
    let expected_casm = serde_json::from_str::<CasmContractClass>(casm_json).unwrap();
    let (reader, mut writer) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_casm(ClassHash::default(), &expected_casm)
        .unwrap()
        .commit()
        .unwrap();

    let casm = reader.begin_ro_txn().unwrap().get_casm(ClassHash::default()).unwrap().unwrap();
    assert_eq!(casm, expected_casm);
}
