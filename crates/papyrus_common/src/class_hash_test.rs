use itertools::Itertools;
use pretty_assertions::assert_eq;
use starknet_api::class_hash;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::StarkHash;
use starknet_api::state::ContractClass;
use test_utils::read_json_file;

use crate::class_hash::{
    calculate_class_hash,
    calculate_deprecated_class_hash,
    ContractClassForHashing,
    ProgramForHashing,
};

#[test]
fn class_hash() {
    let class: ContractClass = serde_json::from_value(read_json_file("class.json")).unwrap();
    let expected_class_hash =
        class_hash!("0x29927c8af6bccf3f6fda035981e765a7bdbf18a2dc0d630494f8758aa908e2b");
    let calculated_class_hash = calculate_class_hash(&class);
    assert_eq!(calculated_class_hash, expected_class_hash);
}

#[test]
fn deprecated_class_hash() {
    let mut deprecated_class: DeprecatedContractClass =
        serde_json::from_value(read_json_file("deprecated_class.json")).unwrap();
    let expected_class_hash =
        class_hash!("0x07b5e991587f0c59db1c4c4ff9b26fa8ec49198ca6d8a82823cc2c6177d918fa");
    let calculated_class_hash = calculate_deprecated_class_hash(&mut deprecated_class).unwrap();
    assert_eq!(calculated_class_hash, expected_class_hash);
}

#[test]
fn deprecated_class_hash_missing_compiler_version() {
    let mut deprecated_class: DeprecatedContractClass =
        serde_json::from_value(read_json_file("deprecated_class_missing_compiler_version.json"))
            .unwrap();
    let expected_class_hash =
        class_hash!("0x025ec026985a3bf9d0cc1fe17326b245dfdc3ff89b8fde106542a3ea56c5a918");
    let calculated_class_hash = calculate_deprecated_class_hash(&mut deprecated_class).unwrap();
    assert_eq!(calculated_class_hash, expected_class_hash);
}

#[test]
fn deprecated_class_hash_with_missing_fields() {
    let mut deprecated_class: DeprecatedContractClass =
        serde_json::from_value(read_json_file("deprecated_class_missing_fields.json")).unwrap();
    let expected_class_hash =
        class_hash!("0x07319e2f01b0947afd86c0bb0e95029551b32f6dc192c47b2e8b08415eebbc25");
    let calculated_class_hash = calculate_deprecated_class_hash(&mut deprecated_class).unwrap();
    assert_eq!(calculated_class_hash, expected_class_hash);
}

// this property is leaned on and the default implementation of serde_json works like
// this. serde_json has a feature called "preserve_order" which could get enabled by
// accident, and it would destroy the ability to compute_class_hash.
#[test]
fn serde_json_value_sorts_maps() {
    let input = r#"{"foo": 1, "bar": 2}"#;
    let parsed = serde_json::from_str::<serde_json::Value>(input).unwrap();
    let output = serde_json::to_string(&parsed).unwrap();

    assert_eq!(output, r#"{"bar":2,"foo":1}"#);
}

#[test]
fn deprecated_class_serialization_for_hashing() {
    // A class with unsorted abi entries and hints to check that the serialization is sorted.
    let deprecated_class = starknet_api::deprecated_contract_class::ContractClass {
        abi: Some(vec![
            starknet_api::deprecated_contract_class::ContractClassAbiEntry::Constructor(
                starknet_api::deprecated_contract_class::FunctionAbiEntry::<
                    starknet_api::deprecated_contract_class::ConstructorType,
                > {
                    name: "constructor".to_string(),
                    inputs: vec![starknet_api::deprecated_contract_class::TypedParameter {
                        name: "implementation".to_string(),
                        r#type: "felt".to_string(),
                    }],
                    outputs: vec![],
                    state_mutability: None,
                    r#type: starknet_api::deprecated_contract_class::ConstructorType::Constructor,
                },
            ),
        ]),
        entry_points_by_type: [(
            starknet_api::deprecated_contract_class::EntryPointType::Constructor,
            vec![],
        )]
        .into(),
        program: starknet_api::deprecated_contract_class::Program {
            hints: serde_json::json!({
            "12": [
                {
                    "accessible_scopes": [
                        "starkware.cairo.common.memcpy",
                        "starkware.cairo.common.memcpy.memcpy"
                    ],
                    "code": "vm_enter_scope({'n': ids.len})",
                    "flow_tracking_data": {
                        "ap_tracking": {
                            "group": 2,
                            "offset": 0
                        },
                        "reference_ids": {
                            "starkware.cairo.common.memcpy.memcpy.len": 0
                        }
                    }
                }
            ],
            "0": [
                {
                    "accessible_scopes": [
                        "starkware.cairo.common.alloc",
                        "starkware.cairo.common.alloc.alloc"
                    ],
                    "code": "memory[ap] = segments.add()",
                    "flow_tracking_data": {
                        "ap_tracking": {
                            "group": 0,
                            "offset": 0
                        },
                        "reference_ids": {}
                    }
                }
            ],
            "20": [
                {
                    "accessible_scopes": [
                        "starkware.cairo.common.memcpy",
                        "starkware.cairo.common.memcpy.memcpy"
                    ],
                    "code": "n -= 1\nids.continue_copying = 1 if n > 0 else 0",
                    "flow_tracking_data": {
                        "ap_tracking": {
                            "group": 2,
                            "offset": 5
                        },
                        "reference_ids": {
                            "starkware.cairo.common.memcpy.memcpy.continue_copying": 1
                        }
                    }
                }
            ]}),
            ..Default::default()
        },
    };

    let for_hashing = ContractClassForHashing {
        abi: &deprecated_class.abi,
        program: ProgramForHashing {
            attributes: &deprecated_class.program.attributes,
            builtins: &deprecated_class.program.builtins,
            compiler_version: &deprecated_class.program.compiler_version,
            data: &deprecated_class.program.data,
            debug_info: &deprecated_class.program.debug_info,
            hints: &deprecated_class.program.hints,
            identifiers: &deprecated_class.program.identifiers,
            main_scope: &deprecated_class.program.main_scope,
            prime: &deprecated_class.program.prime,
            reference_manager: &deprecated_class.program.reference_manager,
        },
    };

    let serialized = serde_json::to_value(for_hashing).unwrap();
    let class_mapping = serialized.as_object().unwrap();

    // Check that the keys of abi entries are sorted lexicographically.
    let abi_entry =
        class_mapping.get("abi").unwrap().as_array().unwrap().first().unwrap().as_object().unwrap();
    for (k1, k2) in abi_entry.keys().tuple_windows() {
        assert!(k1 <= k2);
    }

    // Check that the entry points are skipped in the serialization.
    assert!(class_mapping.get("entry_points_by_type").is_none());

    // Check that the hints are sorted by their index integer value (not lexicographically).
    let hints = class_mapping.get("program").unwrap().get("hints").unwrap().as_object().unwrap();
    for (k1, k2) in hints.keys().tuple_windows() {
        assert!(k1.parse::<u32>().unwrap() <= k2.parse::<u32>().unwrap());
    }
}
