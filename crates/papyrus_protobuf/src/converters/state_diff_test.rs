use test_utils::{get_rng, GetTestInstance};

use crate::sync::{
    ContractDiff,
    DataOrFin,
    DeclaredClass,
    DeprecatedDeclaredClass,
    Query,
    StateDiffChunk,
    StateDiffQuery,
};

#[test]
fn convert_state_diff_chunk_contract_diff_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let state_diff_chunk = StateDiffChunk::ContractDiff(ContractDiff::get_test_instance(&mut rng));

    let data = DataOrFin(Some(state_diff_chunk));
    let bytes_data = Vec::<u8>::from(data.clone());
    let res_data = DataOrFin::try_from(bytes_data).unwrap();
    assert_eq!(data, res_data);
}

#[test]
fn convert_state_diff_chunk_declared_class_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let state_diff_chunk =
        StateDiffChunk::DeclaredClass(DeclaredClass::get_test_instance(&mut rng));

    let data = DataOrFin(Some(state_diff_chunk));
    let bytes_data = Vec::<u8>::from(data.clone());
    let res_data = DataOrFin::try_from(bytes_data).unwrap();
    assert_eq!(data, res_data);
}

#[test]
fn convert_state_diff_chunk_deprecated_declared_class_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let state_diff_chunk = StateDiffChunk::DeprecatedDeclaredClass(
        DeprecatedDeclaredClass::get_test_instance(&mut rng),
    );

    let data = DataOrFin(Some(state_diff_chunk));
    let bytes_data = Vec::<u8>::from(data.clone());
    let res_data = DataOrFin::try_from(bytes_data).unwrap();
    assert_eq!(data, res_data);
}

#[test]
fn convert_fin_state_diff_chunk_to_vec_u8_and_back() {
    let data = DataOrFin::<StateDiffChunk>(None);
    let bytes_data = Vec::<u8>::from(data.clone());
    let res_data = DataOrFin::try_from(bytes_data).unwrap();
    assert_eq!(data, res_data);
}

#[test]
fn convert_state_diff_query_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let state_diff_query = StateDiffQuery(Query::get_test_instance(&mut rng));

    let bytes_data = Vec::<u8>::from(state_diff_query.clone());
    let res_data = StateDiffQuery::try_from(bytes_data).unwrap();
    assert_eq!(state_diff_query, res_data);
}
