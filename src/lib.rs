pub mod gateway;
pub mod starknet;
pub mod starknet_client;
pub mod storage;
pub mod sync;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
