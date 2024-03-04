use prost::Message;
use starknet_api::block::BlockHeader;

use crate::db_executor::Data;
use crate::protobuf_messages::protobuf;

#[test]
fn data_to_protobuf_to_bytes_and_back() {
    let data = Data::BlockHeaderAndSignature {
        header: BlockHeader { ..Default::default() },
        signatures: vec![],
    };
    dbg!(&data);
    let mut data_bytes: Vec<u8> = vec![];
    <Data as Into<protobuf::BlockHeadersResponse>>::into(data.clone())
        .encode(&mut data_bytes)
        .unwrap();
    let res_data: Data =
        protobuf::BlockHeadersResponse::decode(&data_bytes[..]).unwrap().try_into().unwrap();
    assert_eq!(res_data, data);
}
