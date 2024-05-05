#[cfg(test)]
#[path = "class_hash_test.rs"]
mod class_hash_test;
use std::num::ParseIntError;

use itertools::Itertools;
use lazy_static::lazy_static;
use serde::{Serialize, Serializer};
use sha3::Digest;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass,
    EntryPointType as DeprecatedEntryPointType,
    Program as DeprecatedContractClassProgram,
};
use starknet_api::hash::{
    pedersen_hash_array,
    poseidon_hash_array,
    PoseidonHash,
    StarkFelt,
    StarkHash,
};
use starknet_api::state::{ContractClass, EntryPointType};
use starknet_crypto::FieldElement;

use crate::deprecated_class_abi::PythonJsonFormatter;
use crate::usize_into_felt;

lazy_static! {
    static ref SIERRA_API_VERSION: StarkFelt = StarkFelt::from(
        FieldElement::from_byte_slice_be(b"CONTRACT_CLASS_V0.1.0")
            .expect("CONTRACT_CLASS_V0.1.0 is valid StarkFelt."),
    );
}

const DEPRECATED_CLASS_API_VERSION: StarkFelt = StarkFelt::ZERO;

/// An error that occurs when calculating the hash of a deprecated contract class.
#[derive(Debug, thiserror::Error)]
pub enum DeprecatedClassHashCalculationError {
    #[error("{0}")]
    BadProgramJson(&'static str),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}

/// Calculates the hash of a contract class.
// Based on Pathfinder code (the starknet.io doc is incorrect).
pub fn calculate_class_hash(class: &ContractClass) -> ClassHash {
    let external_entry_points_hash = entry_points_hash(class, &EntryPointType::External);
    let l1_handler_entry_points_hash = entry_points_hash(class, &EntryPointType::L1Handler);
    let constructor_entry_points_hash = entry_points_hash(class, &EntryPointType::Constructor);
    let abi_hash = abi_hash(&class.abi);
    let program_hash = poseidon_hash_array(class.sierra_program.as_slice());

    let class_hash = poseidon_hash_array(&[
        *SIERRA_API_VERSION,
        external_entry_points_hash.0,
        l1_handler_entry_points_hash.0,
        constructor_entry_points_hash.0,
        abi_hash,
        program_hash.0,
    ]);
    // TODO: Modify ClassHash Be be PoseidonHash instead of StarkFelt.
    ClassHash(class_hash.0)
}

/// Calculates the hash of a deprecated contract class.
/// Note: This function modifies the contract class in place for backwards compatibility.
// Based on Pathfinder code (the starknet.io doc is incorrect).
pub fn calculate_deprecated_class_hash(
    class: &mut DeprecatedContractClass,
) -> Result<ClassHash, DeprecatedClassHashCalculationError> {
    let external_entry_points_hash =
        deprecated_entry_points_hash(class, &DeprecatedEntryPointType::External);
    let l1_handler_entry_points_hash =
        deprecated_entry_points_hash(class, &DeprecatedEntryPointType::L1Handler);
    let constructor_entry_points_hash =
        deprecated_entry_points_hash(class, &DeprecatedEntryPointType::Constructor);
    let builtins_hash = builtins_hash(&class.program.builtins)?;
    // Modifies the program in place.
    let program_hash = deprecated_program_hash(class)?;
    let bytecode_hash = bytecode_hash(&class.program.data)?;

    Ok(ClassHash(pedersen_hash_array(&[
        DEPRECATED_CLASS_API_VERSION,
        external_entry_points_hash,
        l1_handler_entry_points_hash,
        constructor_entry_points_hash,
        builtins_hash,
        program_hash,
        bytecode_hash,
    ])))
}

fn entry_points_hash(class: &ContractClass, entry_point_type: &EntryPointType) -> PoseidonHash {
    poseidon_hash_array(
        class
            .entry_points_by_type
            .get(entry_point_type)
            .unwrap_or(&vec![])
            .iter()
            .flat_map(|ep| [ep.selector.0, usize_into_felt(ep.function_idx.0)])
            .collect::<Vec<_>>()
            .as_slice(),
    )
}

fn abi_hash(abi: &str) -> StarkFelt {
    let abi_keccak = sha3::Keccak256::default().chain_update(abi.as_bytes()).finalize();
    truncated_keccak(abi_keccak.into())
}

fn deprecated_entry_points_hash(
    class: &DeprecatedContractClass,
    entry_point_type: &DeprecatedEntryPointType,
) -> StarkHash {
    pedersen_hash_array(
        class
            .entry_points_by_type
            .get(entry_point_type)
            .unwrap_or(&vec![])
            .iter()
            .flat_map(|ep| [ep.selector.0, usize_into_felt(ep.offset.0)])
            .collect::<Vec<_>>()
            .as_slice(),
    )
}

fn builtins_hash(
    builtins_json: &serde_json::Value,
) -> Result<StarkFelt, DeprecatedClassHashCalculationError> {
    let Some(builtins) = builtins_json.as_array() else {
        return Err(DeprecatedClassHashCalculationError::BadProgramJson(
            "Builtins expected to be an array of strings.",
        ));
    };
    // Builtins are an array of strings, for example: ["pedersen", "range_check"]. We convert the
    // strings to bytes and from bytes to StarkFelt.
    // TODO(yair): Consider deserializing to a builtin enum.
    let builtins_as_felts = builtins
        .iter()
        .map(|builtin_json| -> Result<StarkFelt, DeprecatedClassHashCalculationError> {
            let builtin_as_bytes = builtin_json
                .as_str()
                .ok_or(DeprecatedClassHashCalculationError::BadProgramJson(
                    "Builtin expected to be a string.",
                ))?
                .as_bytes();
            let Ok(as_field_element) = FieldElement::from_byte_slice_be(builtin_as_bytes) else {
                return Err(DeprecatedClassHashCalculationError::BadProgramJson(
                    "Failed to convert builtin to a field element.",
                ));
            };
            Ok(StarkFelt::from(as_field_element))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(pedersen_hash_array(builtins_as_felts.as_slice()))
}

fn deprecated_program_hash(
    class: &mut DeprecatedContractClass,
) -> Result<StarkFelt, DeprecatedClassHashCalculationError> {
    fix_old_version_program(&mut class.program)?;
    let pythonic_class_serializtion = pythonic_serialize(class)?;
    let class_keccak =
        sha3::Keccak256::default().chain_update(pythonic_class_serializtion.as_bytes()).finalize();
    Ok(truncated_keccak(class_keccak.into()))
}

fn bytecode_hash(
    bytecode_json: &serde_json::Value,
) -> Result<StarkFelt, DeprecatedClassHashCalculationError> {
    let Some(bytecode) = bytecode_json.as_array() else {
        return Err(DeprecatedClassHashCalculationError::BadProgramJson(
            "Expecting the bytecode to be an array of string represented felts.",
        ));
    };
    let bytecode_as_felts = bytecode
        .iter()
        .map(|j| {
            let Some(as_str) = j.as_str() else {
                return Err(DeprecatedClassHashCalculationError::BadProgramJson(
                    "Expecting each bytecode entry to be a string.",
                ));
            };
            let Ok(felt) = StarkFelt::try_from(as_str) else {
                return Err(DeprecatedClassHashCalculationError::BadProgramJson(
                    "Expecting each bytecode entry to be a string represented felt.",
                ));
            };
            Ok(felt)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(pedersen_hash_array(bytecode_as_felts.as_slice()))
}

fn fix_old_version_program(
    program: &mut DeprecatedContractClassProgram,
) -> Result<(), DeprecatedClassHashCalculationError> {
    program.debug_info = serde_json::Value::Null;
    if let Some(program_attributes) = program.attributes.as_array_mut() {
        for attr_json in program_attributes.iter_mut() {
            let Some(vals) = attr_json.as_object_mut() else {
                return Err(DeprecatedClassHashCalculationError::BadProgramJson(
                    "Program json is not an object",
                ));
            };

            // Cairo 0.8 added "accessible_scopes" and "flow_tracking_data" attribute fields, which
            // were not present in older contracts. They present as null / empty for
            // older contracts and should not be included in the hash calculation in
            // these cases.
            match vals.get_mut("accessible_scopes") {
                Some(serde_json::Value::Array(array)) => {
                    if array.is_empty() {
                        vals.remove("accessible_scopes");
                    }
                }
                Some(_other) => {
                    return Err(DeprecatedClassHashCalculationError::BadProgramJson(
                        r#"A program's attribute["accessible_scopes"] was not an array type."#,
                    ));
                }
                None => {}
            }
            if let Some(serde_json::Value::Null) = vals.get_mut("flow_tracking_data") {
                vals.remove("flow_tracking_data");
            }
        }
    }

    if program.compiler_version.is_null() {
        json_traversal(&mut program.identifiers, add_extra_space_to_cairo_named_tuples);
        json_traversal(&mut program.reference_manager, add_extra_space_to_cairo_named_tuples);
    }

    Ok(())
}

fn add_extra_space_to_cairo_named_tuples(json: &mut serde_json::Value) {
    let Some(obj) = json.as_object_mut() else {
        return;
    };
    const KEYS_TO_ADD_SPACE: [&str; 2] = ["cairo_type", "value"];
    for key in KEYS_TO_ADD_SPACE {
        if let Some(serde_json::Value::String(v)) = obj.get_mut(key) {
            let new_value = v.as_str().replace(": ", " : ").replace("  :", " :");
            if new_value != *v {
                *v = new_value;
            }
        }
    }
}

// TODO(yair): Figure out why exactly this function is needed.
// Python code masks with (2**250 - 1) which starts 0x03 and is followed by 31 0xff in big endian.
// Truncation is needed not to overflow the field element.
fn truncated_keccak(mut plain: [u8; 32]) -> StarkFelt {
    plain[0] &= 0x03;
    StarkFelt::new_unchecked(plain)
}

// Traverses the JSON (entering arrays and maps values) and applies the function to each value.
fn json_traversal(json: &mut serde_json::Value, f: fn(&mut serde_json::Value)) {
    f(json);
    match json {
        serde_json::Value::Object(obj) => {
            obj.values_mut().for_each(|value| json_traversal(value, f));
        }
        serde_json::Value::Array(arr) => {
            arr.iter_mut().for_each(|value| json_traversal(value, f));
        }
        _ => {}
    }
}

fn pythonic_serialize(
    class: &DeprecatedContractClass,
) -> Result<String, DeprecatedClassHashCalculationError> {
    let mut string_buffer = vec![];
    let mut serializer =
        serde_json::Serializer::with_formatter(&mut string_buffer, PythonJsonFormatter);

    let class_for_hashing = ContractClassForHashing {
        abi: &class.abi,
        program: ProgramForHashing {
            attributes: &class.program.attributes,
            builtins: &class.program.builtins,
            compiler_version: &class.program.compiler_version,
            data: &class.program.data,
            debug_info: &class.program.debug_info,
            hints: &class.program.hints,
            identifiers: &class.program.identifiers,
            main_scope: &class.program.main_scope,
            prime: &class.program.prime,
            reference_manager: &class.program.reference_manager,
        },
    };

    class_for_hashing.serialize(&mut serializer)?;
    String::from_utf8(string_buffer).map_err(|e| e.into())
}

// A type for skipping the entry points when hashing the contract class.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub(crate) struct ContractClassForHashing<'a> {
    // Starknet does not verify the abi. If we can't parse it, we set it to None.
    pub abi: &'a Option<Vec<starknet_api::deprecated_contract_class::ContractClassAbiEntry>>,
    pub program: ProgramForHashing<'a>,
}

// A type for skipping empty fields when hashing the contract class.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub(crate) struct ProgramForHashing<'a> {
    #[serde(skip_serializing_if = "is_empty")]
    pub attributes: &'a serde_json::Value,
    pub builtins: &'a serde_json::Value,
    #[serde(skip_serializing_if = "is_empty")]
    pub compiler_version: &'a serde_json::Value,
    pub data: &'a serde_json::Value,
    pub debug_info: &'a serde_json::Value,
    #[serde(serialize_with = "serialize_hints_sorted")]
    pub hints: &'a serde_json::Value,
    pub identifiers: &'a serde_json::Value,
    pub main_scope: &'a serde_json::Value,
    pub prime: &'a serde_json::Value,
    pub reference_manager: &'a serde_json::Value,
}

fn is_empty(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Array(arr) => arr.is_empty(),
        serde_json::Value::Object(obj) => obj.is_empty(),
        serde_json::Value::Null => true,
        _ => false,
    }
}

// TODO(yair): Make this function public in sn_api.
// Serialize hints as a sorted mapping for correct hash computation.
fn serialize_hints_sorted<S>(hints: &serde_json::Value, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if hints.is_null() {
        return serializer.serialize_none();
    }
    let hints_map =
        hints.as_object().ok_or(serde::ser::Error::custom("Hints are not a mapping."))?;
    serializer.collect_map(
        hints_map
            .iter()
            // Parse the keys as integers and sort them.
            .map(|(k, v)| Ok((k.parse::<u32>()?, v)))
            .collect::<Result<Vec<_>, ParseIntError>>()
            .map_err(serde::ser::Error::custom)?
            .iter()
            .sorted_by_key(|(k, _v)| *k)
            // Convert the keys back to strings.
            .map(|(k, v)| (k.to_string(), v)),
    )
}
