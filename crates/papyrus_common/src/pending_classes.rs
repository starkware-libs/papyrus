use std::collections::HashMap;
use std::sync::Arc;

use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::ContractClass;

pub trait PendingClassesTrait {
    // TODO(shahak) Return an Arc to avoid cloning the class. This requires to re-implement
    // From/TryFrom for various structs in a way that the input is passed by reference.
    fn get_class(&self, class_hash: ClassHash) -> Option<ApiContractClass>;

    fn add_class(&mut self, class_hash: ClassHash, class: ApiContractClass);

    // TODO(shahak) Return an Arc to avoid cloning the class. This requires to re-implement
    // From/TryFrom for various structs in a way that the input is passed by reference.
    fn get_compiled_class(&self, class_hash: ClassHash) -> Option<CasmContractClass>;

    fn add_compiled_class(&mut self, class_hash: ClassHash, compiled_class: CasmContractClass);

    fn clear(&mut self);
}

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct PendingClasses {
    // Putting the contracts inside Arc so we won't have to clone them when we clone the entire
    // PendingClasses struct.
    pub classes: HashMap<ClassHash, Arc<ApiContractClass>>,
    pub compiled_classes: HashMap<ClassHash, Arc<CasmContractClass>>,
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
        self.classes.get(&class_hash).map(|class| (**class).clone())
    }

    fn add_class(&mut self, class_hash: ClassHash, class: ApiContractClass) {
        self.classes.insert(class_hash, Arc::new(class));
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> Option<CasmContractClass> {
        self.compiled_classes.get(&class_hash).map(|compiled_class| (**compiled_class).clone())
    }

    fn add_compiled_class(&mut self, class_hash: ClassHash, compiled_class: CasmContractClass) {
        self.compiled_classes.insert(class_hash, Arc::new(compiled_class));
    }

    fn clear(&mut self) {
        self.classes.clear();
        self.compiled_classes.clear();
    }
}
