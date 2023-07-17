use camelpaste::paste;
use starknet_api::deprecated_contract_class::{
    EventAbiEntry, FunctionAbiEntryWithType, StructAbiEntry,
};
use test_utils::{get_rng, GetTestInstance};

use super::deprecated_contract_class::{ContractClassAbiEntryType, ContractClassAbiEntryWithType};

macro_rules! test_ContractClassAbiEntryType_from_FunctionAbiEntryType {
    ($variant:ident) => {
        paste! {
            #[tokio::test]
            #[allow(non_snake_case)]
            async fn [< ContractClassAbiEntryType_from_FunctionAbiEntryType_ $variant:lower>]() {
                let _: ContractClassAbiEntryType =
                starknet_api::deprecated_contract_class::FunctionAbiEntryType::$variant
                    .try_into()
                    .unwrap();
            }
        }
    };
}
test_ContractClassAbiEntryType_from_FunctionAbiEntryType!(Constructor);
test_ContractClassAbiEntryType_from_FunctionAbiEntryType!(L1Handler);
test_ContractClassAbiEntryType_from_FunctionAbiEntryType!(Function);

#[tokio::test]
async fn test_contractclassabientrywithtype_from_api_contractclassabientry() {
    let mut rng = get_rng();
    let _: ContractClassAbiEntryWithType =
        starknet_api::deprecated_contract_class::ContractClassAbiEntry::Event(
            EventAbiEntry::get_test_instance(&mut rng),
        )
        .try_into()
        .unwrap();
    let _: ContractClassAbiEntryWithType =
        starknet_api::deprecated_contract_class::ContractClassAbiEntry::Function(
            FunctionAbiEntryWithType::get_test_instance(&mut rng),
        )
        .try_into()
        .unwrap();
    let _: ContractClassAbiEntryWithType =
        starknet_api::deprecated_contract_class::ContractClassAbiEntry::Struct(
            StructAbiEntry::get_test_instance(&mut rng),
        )
        .try_into()
        .unwrap();
}

// macro to generate a test that creates a ContractClassAbiEntry with a variant based on the given
// variant input and call try_into().unwrap()
macro_rules! test_contract_class_abi_entry_with_type {
    ($variant:ident, $variant_inner:ident) => {
        paste! {
            #[tokio::test]
            #[allow(non_snake_case)]
            async fn [<ContractClassAbiEntryWithType_from_api_ContractClassAbiEntry_ $variant:lower>]() {
                let mut rng = get_rng();
                let _: ContractClassAbiEntryWithType =
                    starknet_api::deprecated_contract_class::ContractClassAbiEntry::$variant(
                        $variant_inner::get_test_instance(&mut rng),
                    )
                    .try_into()
                    .unwrap();
            }
        }
    };
}

test_contract_class_abi_entry_with_type!(Event, EventAbiEntry);
test_contract_class_abi_entry_with_type!(Function, FunctionAbiEntryWithType);
test_contract_class_abi_entry_with_type!(Struct, StructAbiEntry);
