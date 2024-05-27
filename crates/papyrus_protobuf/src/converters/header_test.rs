use starknet_api::block::{BlockHeader, BlockNumber};

use crate::protobuf;
use crate::sync::{BlockHashOrNumber, Direction, HeaderQuery, Query, SignedBlockHeader};

#[test]
fn block_header_to_protobuf_and_back() {
    let data = SignedBlockHeader {
        // TODO(shahak): Remove state_diff_length from here once we correctly deduce if it should
        // be None or Some.
        block_header: BlockHeader { state_diff_length: Some(0), ..Default::default() },
        signatures: vec![],
    };
    dbg!(&data);
    let proto_data = protobuf::BlockHeadersResponse::from(Some(data.clone()));

    let res_data = Option::<SignedBlockHeader>::try_from(proto_data).unwrap().unwrap();
    assert_eq!(res_data, data);
}

#[test]
fn fin_to_protobuf_and_back() {
    let proto_data = protobuf::BlockHeadersResponse::from(None);

    let res_data = Option::<SignedBlockHeader>::try_from(proto_data).unwrap();
    assert!(res_data.is_none());
}

#[test]
fn header_query_to_bytes_and_back() {
    let query = HeaderQuery(Query {
        start_block: BlockHashOrNumber::Number(BlockNumber(0)),
        direction: Direction::Forward,
        limit: 1,
        step: 1,
    });

    let bytes = Vec::<u8>::from(query.clone());
    let res_query = HeaderQuery::try_from(bytes).unwrap();
    assert_eq!(query, res_query);
}
