use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use papyrus_common::pending_classes::{PendingClass, PendingClasses};
use pretty_assertions::assert_eq;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_client::reader::{
    ContractClass as Sierra,
    DeclaredClassHashEntry,
    GenericContractClass,
    MockStarknetReader,
    PendingData,
};
use test_utils::GetTestInstance;
use tokio::sync::RwLock;

use crate::sources::pending::{GenericPendingSource, PendingSourceTrait};

#[tokio::test]
async fn get_pending_data() {
    let mut client_mock = MockStarknetReader::new();

    // We need to perform all the mocks before moving the mock into pending_source.
    // TODO(dvir): use pending_data which isn't the default.
    client_mock.expect_pending_data().times(1).returning(|| Ok(Some(PendingData::default())));

    let pending_source = GenericPendingSource { starknet_client: Arc::new(client_mock) };

    let pending_data = pending_source.get_pending_data().await.unwrap();
    assert_eq!(pending_data, PendingData::default());
}

#[tokio::test]
async fn add_pending_deprecated_class() {
    let mut client_mock = MockStarknetReader::new();
    let mut rng = test_utils::get_rng();
    let class = DeprecatedContractClass::get_test_instance(&mut rng);

    // We need to perform all the mocks before moving the mock into pending_source.
    let cloned_class = class.clone();
    client_mock.expect_class_by_hash().times(1).returning(move |_| {
        Ok(Some(GenericContractClass::Cairo0ContractClass(cloned_class.clone())))
    });

    let pending_classes = Arc::new(RwLock::new(PendingClasses::new()));
    let pending_source = GenericPendingSource { starknet_client: Arc::new(client_mock) };

    // Empty pending classes.
    assert_eq!(pending_classes.read().await.deref(), &PendingClasses::new());

    pending_source
        .add_pending_deprecated_class(ClassHash::default(), pending_classes.clone())
        .await
        .unwrap();

    let expected = HashMap::from([(ClassHash::default(), PendingClass::Cairo0(class))]);
    assert_eq!(pending_classes.read().await.classes, expected);
}

#[tokio::test]
async fn add_pending_class() {
    let mut client_mock = MockStarknetReader::new();
    let mut rng = test_utils::get_rng();

    // TODO(dvir): consider use not the default value.
    let sierra = Sierra::default();
    let casm = CasmContractClass::get_test_instance(&mut rng);

    // We need to perform all the mocks before moving the mock into pending_source.
    let cloned_casm = casm.clone();
    client_mock
        .expect_compiled_class_by_hash()
        .times(1)
        .returning(move |_| Ok(Some(cloned_casm.clone())));

    let cloned_sierra = sierra.clone();
    client_mock.expect_class_by_hash().times(1).returning(move |_| {
        Ok(Some(GenericContractClass::Cairo1ContractClass(cloned_sierra.clone())))
    });

    let pending_classes = Arc::new(RwLock::new(PendingClasses::new()));
    let pending_source = GenericPendingSource { starknet_client: Arc::new(client_mock) };

    // Empty pending classes.
    assert!(pending_classes.read().await.eq(&PendingClasses::new()));

    pending_source
        .add_pending_class(DeclaredClassHashEntry::default(), pending_classes.clone())
        .await
        .unwrap();

    let expected_classes =
        HashMap::from([(ClassHash::default(), PendingClass::Cairo1(sierra.into()))]);
    let expected_casm = HashMap::from([(ClassHash::default(), casm)]);

    let expected = PendingClasses { classes: expected_classes, casm: expected_casm };

    assert_eq!(pending_classes.read().await.deref(), &expected);
}
