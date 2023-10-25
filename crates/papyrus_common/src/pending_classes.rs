use std::collections::HashMap;

use cairo_lang_starknet::casm_contract_class::CasmContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::ContractClass as SierraContractClass;

pub trait PendingClassesTrait {
    fn get_class(&self, class_hash: ClassHash) -> Option<PendingClass>;

    fn add_class(&mut self, class_hash: ClassHash, class: PendingClass);

    fn get_compiled_class(
        &self,
        compiled_class_hash: CompiledClassHash,
    ) -> Option<CasmContractClass>;

    fn add_compiled_class(
        &mut self,
        class_hash: CompiledClassHash,
        compiled_class: CasmContractClass,
    );

    fn clear(&mut self);
}

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct PendingClasses {
    pub classes: HashMap<ClassHash, PendingClass>,
    pub compiled_classes: HashMap<CompiledClassHash, CasmContractClass>,
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

    fn get_compiled_class(
        &self,
        compiled_class_hash: CompiledClassHash,
    ) -> Option<CasmContractClass> {
        self.compiled_classes.get(&compiled_class_hash).cloned()
    }

    fn add_compiled_class(
        &mut self,
        compiled_class_hash: CompiledClassHash,
        compiled_class: CasmContractClass,
    ) {
        self.compiled_classes.insert(compiled_class_hash, compiled_class);
    }

    fn clear(&mut self) {
        self.classes.clear();
        self.compiled_classes.clear();
    }
}
