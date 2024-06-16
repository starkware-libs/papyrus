use starknet_api::transaction::{Event, TransactionHash};
use test_utils::{get_rng, GetTestInstance};

use crate::sync::DataOrFin;

#[test]
fn convert_event_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let event = Event::get_test_instance(&mut rng);
    let mut rng = get_rng();
    let transaction_hash = TransactionHash::get_test_instance(&mut rng);

    let data = DataOrFin(Some((event, transaction_hash)));
    let bytes_data = Vec::<u8>::from(data.clone());
    let res_data = DataOrFin::try_from(bytes_data).unwrap();
    assert_eq!(data, res_data);
}

#[test]
fn fin_event_to_bytes_and_back() {
    let bytes_data = Vec::<u8>::from(DataOrFin::<(Event, TransactionHash)>(None));

    let res_data = DataOrFin::<(Event, TransactionHash)>::try_from(bytes_data).unwrap();
    assert!(res_data.0.is_none());
}
