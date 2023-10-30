use std::collections::HashMap;

use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::ContractClass;

pub trait PendingClassesTrait {
    fn get_class(&self, class_hash: ClassHash) -> Option<ApiContractClass>;

    fn add_class(&mut self, class_hash: ClassHash, class: ApiContractClass);

    fn get_compiled_class(&self, class_hash: ClassHash) -> Option<CasmContractClass>;

    fn add_compiled_class(&mut self, class_hash: ClassHash, compiled_class: CasmContractClass);

    fn clear(&mut self);
}

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct PendingClasses {
    pub classes: HashMap<ClassHash, ApiContractClass>,
    pub compiled_classes: HashMap<ClassHash, CasmContractClass>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum ApiContractClass {
    DeprecatedContractClass(DeprecatedContractClass),
    ContractClass(ContractClass),
}

impl ApiContractClass {
    pub fn into_cairo0(self) -> Option<DeprecatedContractClass> {
        match self {
            Self::DeprecatedContractClass(class) => Some(class),
            _ => None,
        }
    }

    pub fn into_cairo1(self) -> Option<ContractClass> {
        match self {
            Self::ContractClass(class) => Some(class),
            _ => None,
        }
    }
}

impl PendingClassesTrait for PendingClasses {
    fn get_class(&self, class_hash: ClassHash) -> Option<ApiContractClass> {
        self.classes.get(&class_hash).cloned()
    }

    fn add_class(&mut self, class_hash: ClassHash, class: ApiContractClass) {
        self.classes.insert(class_hash, class);
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> Option<CasmContractClass> {
        self.compiled_classes.get(&class_hash).cloned()
    }

    fn add_compiled_class(&mut self, class_hash: ClassHash, compiled_class: CasmContractClass) {
        self.compiled_classes.insert(class_hash, compiled_class);
    }

    fn clear(&mut self) {
        self.classes.clear();
        self.compiled_classes.clear();
    }
}
