use std::collections::HashMap;

use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::ContractClass as SierraContractClass;

pub trait PendingClassesTrait {
    fn get_class(&self, class_hash: ClassHash) -> Option<PendingClass>;

    fn add_class(&mut self, class_hash: ClassHash, class: PendingClass);

    fn get_casm(&self, class_hash: ClassHash) -> Option<CasmContractClass>;

    fn add_casm(&mut self, class_hash: ClassHash, casm: CasmContractClass);

    fn clear(&mut self);
}

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct PendingClasses {
    pub classes: HashMap<ClassHash, PendingClass>,
    pub casm: HashMap<ClassHash, CasmContractClass>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum PendingClass {
    Cairo0(DeprecatedContractClass),
    Cairo1(SierraContractClass),
}

impl PendingClassesTrait for PendingClasses {
    fn get_class(&self, class_hash: ClassHash) -> Option<PendingClass> {
        self.classes.get(&class_hash).cloned()
    }

    fn add_class(&mut self, class_hash: ClassHash, class: PendingClass) {
        self.classes.insert(class_hash, class);
    }

    fn get_casm(&self, class_hash: ClassHash) -> Option<CasmContractClass> {
        self.casm.get(&class_hash).cloned()
    }

    fn add_casm(&mut self, class_hash: ClassHash, casm: CasmContractClass) {
        self.casm.insert(class_hash, casm);
    }

    fn clear(&mut self) {
        self.classes.clear();
        self.casm.clear();
    }
}
