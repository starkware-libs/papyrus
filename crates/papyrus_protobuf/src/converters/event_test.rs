use test_utils::{get_rng, GetTestInstance};

use crate::sync::DataOrFin;

#[test]
fn test_event() {
    let mut rng = get_rng();
    let event = starknet_api::transaction::Event::get_test_instance(&mut rng);
    let mut rng = get_rng();
    let transaction_hash = starknet_api::transaction::TransactionHash::get_test_instance(&mut rng);

    let data = DataOrFin(Some((event, transaction_hash)));
    let bytes_data = Vec::<u8>::from(data.clone());
    let res_data = DataOrFin::try_from(bytes_data).unwrap();
    assert_eq!(data, res_data);
}
