use super::algebra::PedersenHash;

pub struct ContractAddress(PedersenHash);
pub struct ContractHash(PedersenHash);
pub struct ContractCode {}
pub struct StorageAddress(PedersenHash);
pub struct StorageValue(PedersenHash);
