use std::collections::VecDeque;
use std::fmt;
use std::time::Instant;

use papyrus_storage::db::serialization::StorageOps;
use papyrus_storage::db::{get_keys_list, get_value, DbConfig};
use papyrus_storage::open_storage;
use papyrus_storage::state::data::IndexedDeprecatedContractClass;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::Program;
use starknet_api::hash::StarkFelt;
use starknet_api::stark_felt;
use test_utils::read_json_file;

pub fn get_program() -> Program {
    let program_json = read_json_file("program.json");
    let program: Program = serde_json::from_value(program_json).unwrap();
    program
}

// Check the time to perform the function foo.
// Returns the (performance time, foo return value).
pub fn check_time<T, F: FnMut() -> T>(mut foo: F) -> (u128, T) {
    let now = Instant::now();
    let ans = foo();
    (now.elapsed().as_micros(), ans)
}
#[derive(Debug, Default)]
pub struct CompressionResult<T> {
    origin: T,
    serialized: Vec<u8>,
    compressed: Vec<u8>,
    ser_size: usize,
    com_size: usize,
    ser_time: u128,
    com_time: u128,
    des_ser_time: u128,
    des_com_time: u128,
}

impl<T> fmt::Display for CompressionResult<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "ser_size: {}", self.ser_size)?;
        writeln!(f, "com_size: {}", self.com_size)?;
        writeln!(f, "size ratio: {}", self.ser_size as f32 / self.com_size as f32)?;
        writeln!(f, "ser_time: {}", self.ser_time)?;
        writeln!(f, "com_time: {}", self.com_time)?;
        writeln!(f, "des_ser_time: {}", self.des_ser_time)?;
        writeln!(f, "des_com_time: {}", self.des_com_time)
    }
}

impl<T: StorageOps + std::cmp::Eq + std::fmt::Debug> CompressionResult<T> {
    pub fn new(origin: T) -> Self {
        let (com_time, compressed) = check_time(|| origin.serialize_with_compression().unwrap());
        let (ser_time, serialized) = check_time(|| origin.serialize_without_compression().unwrap());

        let mut to_de_compressed = VecDeque::from(compressed.clone());
        let (des_com_time, origin_compressed) =
            check_time(|| T::deserialize_with_compression(&mut to_de_compressed).unwrap());
        let mut to_de_serialized = VecDeque::from(serialized.clone());
        let (des_ser_time, origin_serialized) =
            check_time(|| T::deserialize_without_compression(&mut to_de_serialized).unwrap());
        assert_eq!(origin_compressed, origin);
        assert_eq!(origin_serialized, origin);

        CompressionResult {
            origin,
            ser_size: serialized.len(),
            com_size: compressed.len(),
            serialized,
            compressed,
            ser_time,
            com_time,
            des_ser_time,
            des_com_time,
        }
    }
}

fn get_db_config() -> DbConfig{
    DbConfig {
        path: "./data/SN_MAIN".into(),
        // Same values as the default storage config.
        min_size: 1048576,
        max_size: 1099511627776,
        growth_step: 67108864,
    }
}

const TABLE_NAME: &str="deprecated_declared_classes";

fn main() {    
    let (storage_reader, _storage_writer) = open_storage(get_db_config()).unwrap();
    let keys = get_keys_list::<ClassHash, IndexedDeprecatedContractClass>(
        &storage_reader,
        TABLE_NAME,
        &ClassHash(stark_felt!(
            "0x0"
        )),
    );

    let mut v=vec![];
    for key in keys {
        let value = get_value::<ClassHash, IndexedDeprecatedContractClass>(
            &storage_reader,
            TABLE_NAME,
            &key,
        );
        let cur=CompressionResult::new(value.contract_class.program);
        v.push(cur);
    }

    let total=v.len();
    let mut com_bigger=0;
    let mut des_com_bigger=0;
    for s in v{
        if s.com_size>=s.ser_size{
            com_bigger+=1;
        }
        if s.des_com_time>=s.des_ser_time{
            des_com_bigger+=1;
        }
        println!("{}",s);
    }

    println!("total: {}", total);
    println!("com_bigger: {}", com_bigger);
    println!("des_com_bigger: {}", des_com_bigger);
}