#[cfg(test)]
mod test {

    use jsonrpsee::core::RpcResult;
    use jsonrpsee::server::RpcModule;
    use papyrus_rpc::api::JsonRpcServerTrait;
    use papyrus_rpc::test_utils::raw_call;
    use serde::{Deserialize, Serialize};
    use serde_json::{Map, Value};
    use test_utils::get_test_block;

    #[test]
    fn test_main() {
        let block = get_test_block(10, None, None, None);
        dbg!(block);
        assert!(false, "test runs");
    }
}
