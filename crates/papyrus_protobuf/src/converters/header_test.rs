use test_utils::{get_rng, GetTestInstance};

use crate::sync::{DataOrFin, HeaderQuery, SignedBlockHeader};

#[test]
fn block_header_to_bytes_and_back() {
    let mut rng = get_rng();
    let signed_block_header = SignedBlockHeader::get_test_instance(&mut rng);
    let data = DataOrFin(Some(signed_block_header.clone()));
    let bytes_data = Vec::<u8>::from(data.clone());
    let res_data = DataOrFin::try_from(bytes_data).unwrap();
    assert_eq!(res_data, data);
}

#[test]
fn fin_to_bytes_and_back() {
    let bytes_data = Vec::<u8>::from(DataOrFin::<SignedBlockHeader>(None));

    let res_data = DataOrFin::<SignedBlockHeader>::try_from(bytes_data).unwrap();
    assert!(res_data.0.is_none());
}

#[test]
fn header_query_to_bytes_and_back() {
    let mut rng = get_rng();
    let header_query = HeaderQuery::get_test_instance(&mut rng);
    let bytes = Vec::<u8>::from(header_query.clone());
    let res_query = HeaderQuery::try_from(bytes).unwrap();
    assert_eq!(header_query, res_query);
}
