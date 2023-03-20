use std::collections::{HashMap};

use primitive_types::U256;
use serde::Serialize;
use serde_json::{Value, json, Serializer};
use starknet_api::{hash::StarkFelt, state::EntryPoint, core::ClassHash};
use sha3::Digest;
use starknet_api::state::EntryPointType;

fn sn_keccak(data: &[u8]) -> String{
    let keccak256 = sha3::Keccak256::digest(data);
    let number = U256::from_big_endian(keccak256.as_slice());
    let mask = U256::pow(U256::from(2), U256::from(250)) - U256::from(1);
    let masked_number = number & mask;
    let mut res_bytes:[u8;32] = [0;32];
    masked_number.to_big_endian(&mut res_bytes);
    return format!("0x{}", hex::encode(res_bytes).trim_start_matches('0'));
}

fn entry_points_hash_by_type(entry_points: &HashMap<EntryPointType,Vec<EntryPoint>>, entry_point_type: EntryPointType) -> StarkFelt{
    let felts = entry_points.get(&entry_point_type).unwrap_or(&Vec::<EntryPoint>::new())
        .iter()
        .flat_map(|entry| {
            let selector = entry.selector.0;
            let offset = StarkFelt::from(u64::try_from(entry.offset.0).unwrap());
            return vec![selector, offset];
        }).collect::<Vec<StarkFelt>>();

    return starknet_api::hash::pedersen_hash_array(&felts);
}

fn compute_class_hash_from_json(contract_class: &Value) -> String{
    let mut abi_json = json!({
        "abi": contract_class.get("abi").unwrap_or(&Value::Null),
        "program": contract_class.get("program").unwrap_or(&Value::Null)
    });

    let program_json = abi_json.get_mut("program").expect("msg");
    let debug_info_json = program_json.get_mut("debug_info");
    if debug_info_json.is_some(){
        program_json.as_object_mut().unwrap().insert("debug_info".to_owned(), serde_json::Value::Null);
    }

    let mut new_object = serde_json::Map::<String, Value>::new();
    let res = traverse_and_exclude_recursively(&abi_json, &mut new_object, &|key, value| {
        return 
            (key == "attributes" || key == "accessible_scopes") 
            && value.is_array() 
            && value.as_array().unwrap().is_empty();
    });

    let mut writer = Vec::with_capacity(128);
    let mut serializer = Serializer::with_formatter(&mut writer, json_formatter::StarknetFormatter);
    res.serialize(&mut serializer).unwrap();
    let str_json = unsafe{
        String::from_utf8_unchecked(writer)
    };

    let keccak_result = sn_keccak(str_json.as_bytes());
    return keccak_result;
}

fn entry_points_hash_by_type_from_json(contract_class: &Value, entry_point_type: &str) -> StarkFelt{
    let felts = contract_class
        .get("entry_points_by_type")
        .unwrap_or(&serde_json::Value::Null)
        .get(entry_point_type)
        .unwrap_or(&serde_json::Value::Null)
        .as_array()
        .unwrap_or(&Vec::<serde_json::Value>::new())
        .iter()
        .flat_map(|entry|{
            let selector = get_starkfelt_from_json_unsafe(&entry, "selector");
            let offset = get_starkfelt_from_json_unsafe(&entry, "offset");

            return vec![selector, offset];
        }).collect::<Vec<StarkFelt>>();

    return starknet_api::hash::pedersen_hash_array(&felts);
}

fn get_starkfelt_from_json_unsafe(json: &Value, key: &str) -> StarkFelt{
    StarkFelt::try_from(json.get(key).unwrap().as_str().unwrap()).unwrap()
}

pub fn compute_contract_class_hash_v0(contract_class: &serde_json::Value) -> ClassHash{
    // api version
    let api_version = StarkFelt::try_from(format!("0x{}", hex::encode([0u8])).as_str()).unwrap();

    // external entry points hash
    let external_entry_points_hash = entry_points_hash_by_type_from_json(&contract_class, "EXTERNAL");

    // l1 handler entry points hash
    let l1_entry_points_hash = entry_points_hash_by_type_from_json(&contract_class, "L1_HANDLER");

    // constructor handler entry points hash
    let constructor_entry_points_hash = entry_points_hash_by_type_from_json(&contract_class, "CONSTRUCTOR");

    // builtins hash
    let builtins_encoded = contract_class
        .get("program").unwrap_or(&serde_json::Value::Null)
        .get("builtins").unwrap_or(&serde_json::Value::Null)
        .as_array().unwrap_or(&Vec::<serde_json::Value>::new()).iter().map(|str| {
        let hex_str = str.as_str().unwrap().as_bytes().iter().map(|b| format!("{:02x}", b))
            .collect::<Vec<String>>().join("");
        return format!("0x{}", hex_str);
    }).collect::<Vec<String>>();

    let builtins_encoded_as_felts = builtins_encoded.iter().map(|s| {
        return StarkFelt::try_from(s.as_str()).unwrap();      
    }).collect::<Vec<StarkFelt>>();

    let builtins_hash = starknet_api::hash::pedersen_hash_array(&builtins_encoded_as_felts);

    //hinted class hash
    let hinted_class_hash = compute_class_hash_from_json(&contract_class);

    //program data hash
    let program_data_felts = contract_class
        .get("program").unwrap_or(&Value::Null)
        .get("data").unwrap_or(&Value::Null)
        .as_array().unwrap_or(&Vec::<Value>::new())
        .iter()
        .map(|str| {
            return StarkFelt::try_from(str.as_str().unwrap()).unwrap();
    }).collect::<Vec<StarkFelt>>();

    let program_data_hash = starknet_api::hash::pedersen_hash_array(&program_data_felts);

    return ClassHash(starknet_api::hash::pedersen_hash_array(&vec![
        api_version,
        external_entry_points_hash,
        l1_entry_points_hash,
        constructor_entry_points_hash,
        builtins_hash,
        StarkFelt::try_from(hinted_class_hash.as_str()).unwrap(),
        program_data_hash
    ]));
}

pub fn compute_contract_class_hash(contract_class: &crate::transaction::input::ContractClass) -> ClassHash{
    // api version
    let api_version = StarkFelt::try_from(format!("0x{}", hex::encode([0u8])).as_str()).unwrap();

    // external entry points hash
    let external_entry_points_hash = entry_points_hash_by_type(&contract_class.entry_points_by_type, EntryPointType::External);

    // l1 handler entry points hash
    let l1_entry_points_hash = entry_points_hash_by_type(&contract_class.entry_points_by_type, EntryPointType::L1Handler);

    // constructor handler entry points hash
    let constructor_entry_points_hash = entry_points_hash_by_type(&contract_class.entry_points_by_type, EntryPointType::Constructor);
    
    // builtins hash
    let builtins_encoded = contract_class.program.builtins.as_array().unwrap_or(&Vec::<serde_json::Value>::new()).iter().map(|str| {
        let hex_str = str.as_str().unwrap().as_bytes().iter().map(|b| format!("{:02x}", b))
            .collect::<Vec<String>>().join("");
        return format!("0x{}", hex_str);
    }).collect::<Vec<String>>();

    let builtins_encoded_as_felts = builtins_encoded.iter().map(|s| {
        return StarkFelt::try_from(s.as_str()).unwrap();      
    }).collect::<Vec<StarkFelt>>();

    let builtings_hash = starknet_api::hash::pedersen_hash_array(&builtins_encoded_as_felts);

    //hinted class hash
    let hinted_class_hash = compute_class_hash(&contract_class);
    println!("{}", hinted_class_hash);
    // program data hash
    let program_data_felts = contract_class.program.data.as_array().unwrap_or(&Vec::<Value>::new()).iter().map(|str| {
        return StarkFelt::try_from(str.as_str().unwrap()).unwrap();
    }).collect::<Vec<StarkFelt>>();
    let program_data_hash = starknet_api::hash::pedersen_hash_array(&program_data_felts);

    return ClassHash(starknet_api::hash::pedersen_hash_array(&vec![
        api_version,
        external_entry_points_hash,
        l1_entry_points_hash,
        constructor_entry_points_hash,
        builtings_hash,
        StarkFelt::try_from(hinted_class_hash.as_str()).unwrap(),
        program_data_hash
    ]));
}

fn compute_class_hash(contract_class: &crate::transaction::input::ContractClass) -> String {
    let data: serde_json::Value= serde_json::to_value(&contract_class).unwrap();
    let mut abi_json = json!({
        "abi": data.get("abi"),
        "program": data.get("program")
    });

    let mut program_json = abi_json.get_mut("program").expect("msg");
    let mut debug_info_json = program_json.get_mut("debug_info");
    if debug_info_json.is_some(){
        program_json.as_object_mut().unwrap().insert("debug_info".to_owned(), serde_json::Value::Null);
    }

    let mut new_object = serde_json::Map::<String, Value>::new();
    let res = traverse_and_exclude_recursively(&abi_json, &mut new_object, &|key, value| {
        return 
            (key == "attributes" || key == "accessible_scopes") 
            && value.is_array() 
            && value.as_array().unwrap().is_empty();
    });

    let mut writer = Vec::with_capacity(128);
    let mut serializer = Serializer::with_formatter(&mut writer, json_formatter::StarknetFormatter);
    res.serialize(&mut serializer).unwrap();
    let str_json = unsafe{
        String::from_utf8_unchecked(writer)
    };

    let keccak_result = sn_keccak(str_json.as_bytes());
    return keccak_result;
}

/// because of the preserve_order feature enabled in the serde_json crate
/// removing a key from the object changes the order of the keys
/// When serde_json is not being used with the preserver order feature 
/// deserializing to a serde_json::Value changes the order of the keys
///
/// go through the object by visiting every key and value recursively,
/// and not including them into a new json obj if the condition is met
/// Empty objects are not included
pub fn traverse_and_exclude_recursively<F>(
    value: &Value, 
    new_object: &mut serde_json::Map<String, Value>, 
    condition: &F
) -> serde_json::Value where F: Fn(&String, &Value) -> bool{
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                let mut inner_obj = serde_json::Map::new();

                if condition(key, &value){
                    continue
                }

                let inner_val = traverse_and_exclude_recursively(value, &mut inner_obj, condition);
                
                new_object.insert(key.to_string(), inner_val);
            }

            return Value::Object(new_object.clone());
        }
        // arrays are visited like the objects - recursively
        Value::Array(array) => {
            let mut inner_arr = Vec::<Value>::new();

            for value in array {
                let mut inner_obj = serde_json::Map::new();
                let inner_val = traverse_and_exclude_recursively(value, &mut inner_obj, condition);

                if !(inner_val.is_object() && inner_val.as_object().unwrap().is_empty()){
                    inner_arr.push(inner_val)
                }
            }

            return Value::Array(inner_arr);
        }
        // handle non-object, non-array values
        _ => {
            return value.clone();
        }
    }
}

/// because of the preserve_order feature enabled in the serde_json crate
/// removing a key from the object changes the order of the keys
/// When serde_json is not being used with the preserver order feature 
/// deserializing to a serde_json::Value changes the order of the keys
/// Go through object's top level keys and remove those that pass the condition
pub fn traverse_and_exclude_top_level_keys<F>(
    value: &Value,
    condition: &F
) -> serde_json::Value where F: Fn(&String, &Value) -> bool{
    if !value.is_object(){
        return value.clone();
    }

    let mut new_obj = serde_json::Map::new();

    for (key, value) in value.as_object().unwrap(){
        if condition(key, value) {
            continue;
        }

        new_obj.insert(key.clone(), value.clone());
    }

    return Value::Object(new_obj);
}

pub mod json_formatter{
    use std::io;

    use serde_json::ser::Formatter;

    pub struct StarknetFormatter;

    impl Formatter for StarknetFormatter{
        fn begin_object_value<W>(&mut self, writer: &mut W) -> io::Result<()>
            where
                W: ?Sized + io::Write, {
            writer.write_all(b": ")
        }

        fn begin_object_key<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
            where
                W: ?Sized + io::Write, {
            if first {
                Ok(())
            } else {
                writer.write_all(b", ")
            }
        }

        fn begin_array_value<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
            where
                W: ?Sized + io::Write, {
            if first {
                Ok(())
            } else {
                writer.write_all(b", ")
            }
        }
    }
}