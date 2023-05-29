use std::time::Instant;

use papyrus_storage::compression_utils::*;
use papyrus_storage::db::serialization::StorageSerde;
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

pub fn get_full_contract() -> IndexedDeprecatedContractClass {
    let program_json = read_json_file("indexed_declared_contract.json");
    let program: IndexedDeprecatedContractClass = serde_json::from_value(program_json).unwrap();
    program
}

// Returns the time of execution of f and the return value.
pub fn check_time<T, F: FnMut() -> T>(mut f: F) -> (u128, T) {
    let now = Instant::now();
    let ans = f();
    (now.elapsed().as_micros(), ans)
}

#[derive(Debug, Default)]
pub struct CompressionResult<T> {
    _origin: T,
    _serialized: Vec<u8>,
    _compressed: Vec<u8>,
    ser_size: usize,
    com_size: usize,
    ser_time: u128,
    com_time: u128,
    des_ser_time: u128,
    des_com_time: u128,
}

// Trait for all the function you want to test time.
pub trait TestFunctionPerformance {
    fn serialize_with_compression(&self) -> Vec<u8>;
    fn serialize_without_compression(&self) -> Vec<u8>;
    fn deserialize_with_compression(bytes: &mut &[u8]) -> Self;
    fn deserialize_without_compression(bytes: &mut &[u8]) -> Self;
}

impl<T: StorageSerde> TestFunctionPerformance for T {
    fn serialize_with_compression(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer).unwrap();
        compress(buffer.as_slice()).unwrap()
    }

    fn serialize_without_compression(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        self.serialize_into(&mut buffer).unwrap();
        buffer
    }
    fn deserialize_with_compression(bytes: &mut &[u8]) -> Self {
        let bytes = decompress(bytes).unwrap();
        T::deserialize_from(&mut bytes.as_slice()).unwrap()
    }
    fn deserialize_without_compression(bytes: &mut &[u8]) -> Self {
        T::deserialize_from(bytes).unwrap()
    }
}

impl<T: TestFunctionPerformance> CompressionResult<T> {
    pub fn new(origin: T) -> Self {
        let (com_time, compressed) = check_time(|| origin.serialize_with_compression());
        let (ser_time, serialized) = check_time(|| origin.serialize_without_compression());

        let (des_com_time, _origin_compressed) =
            check_time(|| T::deserialize_with_compression(&mut compressed.as_slice()));
        let (des_ser_time, _origin_serialized) =
            check_time(|| T::deserialize_without_compression(&mut serialized.as_slice()));
        // assert_eq!(origin_compressed, origin);
        // assert_eq!(origin_serialized, origin);

        CompressionResult {
            _origin: origin,
            ser_size: serialized.len(),
            com_size: compressed.len(),
            _serialized: serialized,
            _compressed: compressed,
            ser_time,
            com_time,
            des_ser_time,
            des_com_time,
        }
    }

    pub fn print_fields(&self) {
        println!("====");
        println!("ser_size: {}", self.ser_size);
        println!("com_size: {}", self.com_size);
        println!("ser_time: {}", self.ser_time);
        println!("com_time: {}", self.com_time);
        println!("des_ser_time: {}", self.des_ser_time);
        println!("des_com_time: {}", self.com_size);
        println!("====");
    }
}

pub fn get_db_config() -> DbConfig {
    DbConfig {
        path: "./data/SN_MAIN".into(),
        // Same values as the default storage config.
        min_size: 1048576,
        max_size: 1099511627776,
        growth_step: 67108864,
    }
}

// Key and value type of the table we currently use.
type KeyType = ClassHash;
type ValueType = IndexedDeprecatedContractClass;

// Number of keys to read from the database.
const KEY_LIMIT: usize = 10; //usize::MAX;

#[tokio::main]
async fn main() {
    let table_name = "deprecated_declared_classes";
    // let tra_idx=TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(0));
    let (storage_reader_data, _storage_writer) = open_storage(get_db_config()).unwrap();
    let keys_vec = get_keys_list::<KeyType, ValueType>(
        &storage_reader_data,
        table_name,
        //&(ContractAddress::default(), EventIndex(tra_idx, EventIndexInTransactionOutput(0))),
        //&BlockNumber(0),
        //&tra_idx,
        &ClassHash(stark_felt!("0x0")),
        KEY_LIMIT,
    );

    let mut results = Vec::new();
    for key in keys_vec {
        let value = get_value::<KeyType, ValueType>(&storage_reader_data, table_name, &key);
        let cur = CompressionResult::new(value.contract_class.program);
        results.push(cur);
    }

    let total_values = results.len();
    let mut com_bigger = 0;
    let mut des_com_bigger = 0;
    let mut sum_com = 0;
    let mut sum_ser = 0;
    for s in results.iter() {
        sum_com += s.com_size;
        sum_ser += s.ser_size;
        if s.com_size >= s.ser_size {
            com_bigger += 1;
        }
        if s.des_com_time >= s.des_ser_time {
            des_com_bigger += 1;
        }
        s.print_fields();
    }

    println!("total_values: {}", total_values);
    println!("com_bigger: {}", com_bigger);
    println!("des_com_bigger: {}", des_com_bigger);

    println!();
    println!("sum_ser size: {}", sum_ser);
    println!("sum_com size: {}", sum_com);
    println!("ratio: {}", sum_ser as f64 / sum_com as f64);
}
