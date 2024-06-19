#[cfg(test)]
#[path = "state_diff_test.rs"]
mod state_diff_test;
use indexmap::IndexMap;
use prost::Message;
use starknet_api::core::{ClassHash, CompiledClassHash, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_types_core::felt::Felt;

use super::common::volition_domain_to_enum_int;
use super::ProtobufConversionError;
use crate::sync::{
    ContractDiff,
    DataOrFin,
    DeclaredClass,
    DeprecatedDeclaredClass,
    Query,
    StateDiffChunk,
    StateDiffQuery,
};
use crate::{auto_impl_into_and_try_from_vec_u8, auto_impl_try_from_vec_u8, protobuf};

pub const DOMAIN: DataAvailabilityMode = DataAvailabilityMode::L1;

// TODO(shahak): Remove this once we finish the sync refactor.
impl TryFrom<protobuf::StateDiffsResponse> for DataOrFin<ThinStateDiff> {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::StateDiffsResponse) -> Result<Self, Self::Error> {
        match value.state_diff_message {
            Some(protobuf::state_diffs_response::StateDiffMessage::ContractDiff(contract_diff)) => {
                Ok(DataOrFin(Some(contract_diff.try_into()?)))
            }
            Some(protobuf::state_diffs_response::StateDiffMessage::DeclaredClass(
                declared_class,
            )) => Ok(DataOrFin(Some(declared_class.try_into()?))),
            Some(protobuf::state_diffs_response::StateDiffMessage::Fin(_)) => Ok(DataOrFin(None)),
            None => Err(ProtobufConversionError::MissingField {
                field_description: "StateDiffsResponse::state_diff_message",
            }),
        }
    }
}
auto_impl_try_from_vec_u8!(DataOrFin<ThinStateDiff>, protobuf::StateDiffsResponse);

impl TryFrom<protobuf::StateDiffsResponse> for DataOrFin<StateDiffChunk> {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::StateDiffsResponse) -> Result<Self, Self::Error> {
        match value.state_diff_message {
            Some(protobuf::state_diffs_response::StateDiffMessage::ContractDiff(contract_diff)) => {
                Ok(DataOrFin(Some(StateDiffChunk::ContractDiff(contract_diff.try_into()?))))
            }
            Some(protobuf::state_diffs_response::StateDiffMessage::DeclaredClass(
                declared_class,
            )) => match declared_class.compiled_class_hash.as_ref() {
                Some(_compiled_class_hash) => {
                    Ok(DataOrFin(Some(StateDiffChunk::DeclaredClass(declared_class.try_into()?))))
                }
                None => Ok(DataOrFin(Some(StateDiffChunk::DeprecatedDeclaredClass(
                    declared_class.try_into()?,
                )))),
            },
            Some(protobuf::state_diffs_response::StateDiffMessage::Fin(_)) => Ok(DataOrFin(None)),
            None => Err(ProtobufConversionError::MissingField {
                field_description: "StateDiffsResponse::state_diff_message",
            }),
        }
    }
}

impl From<DataOrFin<StateDiffChunk>> for protobuf::StateDiffsResponse {
    fn from(value: DataOrFin<StateDiffChunk>) -> Self {
        let state_diff_message = match value.0 {
            Some(StateDiffChunk::ContractDiff(contract_diff)) => {
                protobuf::state_diffs_response::StateDiffMessage::ContractDiff(contract_diff.into())
            }
            Some(StateDiffChunk::DeclaredClass(declared_class)) => {
                protobuf::state_diffs_response::StateDiffMessage::DeclaredClass(
                    declared_class.into(),
                )
            }
            Some(StateDiffChunk::DeprecatedDeclaredClass(deprecated_declared_class)) => {
                protobuf::state_diffs_response::StateDiffMessage::DeclaredClass(
                    deprecated_declared_class.into(),
                )
            }
            None => protobuf::state_diffs_response::StateDiffMessage::Fin(protobuf::Fin {}),
        };
        protobuf::StateDiffsResponse { state_diff_message: Some(state_diff_message) }
    }
}

auto_impl_into_and_try_from_vec_u8!(DataOrFin<StateDiffChunk>, protobuf::StateDiffsResponse);

impl TryFrom<protobuf::ContractDiff> for ThinStateDiff {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ContractDiff) -> Result<Self, Self::Error> {
        let contract_address = value
            .address
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "ContractDiff::address",
            })?
            .try_into()?;

        let deployed_contracts = value
            .class_hash
            .map(|hash| {
                Ok::<_, ProtobufConversionError>(IndexMap::from_iter([(
                    contract_address,
                    ClassHash(hash.try_into()?),
                )]))
            })
            .transpose()?
            .unwrap_or_default();

        let storage_diffs = if value.values.is_empty() {
            IndexMap::new()
        } else {
            let storage_values = value
                .values
                .into_iter()
                .map(|stored_value| stored_value.try_into())
                .collect::<Result<IndexMap<StorageKey, Felt>, _>>()?;
            IndexMap::from_iter([(contract_address, storage_values)])
        };

        let nonces = value
            .nonce
            .map(|nonce| {
                Ok::<_, ProtobufConversionError>(IndexMap::from_iter([(
                    contract_address,
                    Nonce(nonce.try_into()?),
                )]))
            })
            .transpose()?
            .unwrap_or_default();

        // TODO(shahak): Use the domain field once Starknet supports volition.

        Ok(ThinStateDiff {
            deployed_contracts,
            storage_diffs,
            nonces,
            // These two fields come from DeclaredClass messages.
            declared_classes: Default::default(),
            deprecated_declared_classes: Default::default(),
            // The p2p specs doesn't separate replaced classes from deployed contracts. In RPC v0.8
            // the node will stop separating them as well. Until then nodes syncing from
            // P2P won't be able to separate replaced classes from deployed contracts correctly
            replaced_classes: Default::default(),
        })
    }
}

impl TryFrom<protobuf::DeclaredClass> for ThinStateDiff {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::DeclaredClass) -> Result<Self, Self::Error> {
        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclaredClass::class_hash",
                })?
                .try_into()?,
        );

        // According to the P2P specs, if compiled_class_hash is missing, the declared class is a
        // cairo-0 class.
        match value.compiled_class_hash {
            Some(compiled_class_hash) => Ok(ThinStateDiff {
                declared_classes: IndexMap::from_iter([(
                    class_hash,
                    CompiledClassHash(compiled_class_hash.try_into()?),
                )]),
                ..Default::default()
            }),
            None => Ok(ThinStateDiff {
                deprecated_declared_classes: vec![class_hash],
                ..Default::default()
            }),
        }
    }
}

impl TryFrom<protobuf::ContractStoredValue> for (StorageKey, Felt) {
    type Error = ProtobufConversionError;
    fn try_from(entry: protobuf::ContractStoredValue) -> Result<Self, Self::Error> {
        let key_felt = Felt::try_from(entry.key.ok_or(ProtobufConversionError::MissingField {
            field_description: "ContractStoredValue::key",
        })?)?;
        let key = StorageKey(key_felt.try_into().map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                // TODO(shahak): Check if the type in the protobuf of the field
                // ContractStoredValue::key should be changed into a PatriciaKey which has a
                // slightly lower bound than Felt.
                type_description: "Felt252",
                value_as_str: format!("{key_felt:?}"),
            }
        })?);
        let value = Felt::try_from(entry.value.ok_or(ProtobufConversionError::MissingField {
            field_description: "ContractStoredValue::value",
        })?)?;
        Ok((key, value))
    }
}

// TODO(shahak): Erase this once network stops using it.
impl TryFrom<protobuf::StateDiffsRequest> for Query {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::StateDiffsRequest) -> Result<Self, Self::Error> {
        Ok(StateDiffQuery::try_from(value)?.0)
    }
}

impl TryFrom<protobuf::StateDiffsRequest> for StateDiffQuery {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::StateDiffsRequest) -> Result<Self, Self::Error> {
        Ok(StateDiffQuery(
            value
                .iteration
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "StateDiffsRequest::iteration",
                })?
                .try_into()?,
        ))
    }
}

// TODO(shahak): Erase this once network stops using it.
impl From<Query> for protobuf::StateDiffsRequest {
    fn from(value: Query) -> Self {
        protobuf::StateDiffsRequest { iteration: Some(value.into()) }
    }
}

impl From<StateDiffQuery> for protobuf::StateDiffsRequest {
    fn from(value: StateDiffQuery) -> Self {
        protobuf::StateDiffsRequest { iteration: Some(value.0.into()) }
    }
}

auto_impl_into_and_try_from_vec_u8!(StateDiffQuery, protobuf::StateDiffsRequest);

impl TryFrom<protobuf::ContractDiff> for ContractDiff {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ContractDiff) -> Result<Self, Self::Error> {
        let contract_address = value
            .address
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "ContractDiff::address",
            })?
            .try_into()?;

        // class_hash can be None if the contract wasn't deployed in this block
        let class_hash = value
            .class_hash
            .map(|class_hash| Ok::<_, ProtobufConversionError>(ClassHash(class_hash.try_into()?)))
            .transpose()?;

        // nonce can be None if it wasn't updated in this block
        let nonce = value
            .nonce
            .map(|nonce| Ok::<_, ProtobufConversionError>(Nonce(nonce.try_into()?)))
            .transpose()?;

        let storage_diffs = value
            .values
            .into_iter()
            .map(|stored_value| stored_value.try_into())
            .collect::<Result<IndexMap<StorageKey, Felt>, _>>()?;

        Ok(ContractDiff { contract_address, class_hash, nonce, storage_diffs })
    }
}

impl From<ContractDiff> for protobuf::ContractDiff {
    fn from(value: ContractDiff) -> Self {
        let contract_address = Some(value.contract_address.into());
        let class_hash = value.class_hash.map(|hash| hash.0.into());
        let nonce = value.nonce.map(|nonce| nonce.0.into());
        let values = value
            .storage_diffs
            .into_iter()
            .map(|(key, value)| protobuf::ContractStoredValue {
                key: Some((*key.0.key()).into()),
                value: Some(value.into()),
            })
            .collect();
        let domain = volition_domain_to_enum_int(DOMAIN);
        protobuf::ContractDiff { address: contract_address, class_hash, nonce, values, domain }
    }
}
impl TryFrom<protobuf::DeclaredClass> for DeclaredClass {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::DeclaredClass) -> Result<Self, Self::Error> {
        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclaredClass::class_hash",
                })?
                .try_into()?,
        );
        let compiled_class_hash = CompiledClassHash(
            value
                .compiled_class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclaredClass::compiled_class_hash",
                })?
                .try_into()?,
        );
        Ok(DeclaredClass { class_hash, compiled_class_hash })
    }
}

impl From<DeclaredClass> for protobuf::DeclaredClass {
    fn from(value: DeclaredClass) -> Self {
        protobuf::DeclaredClass {
            class_hash: Some(value.class_hash.0.into()),
            compiled_class_hash: Some(value.compiled_class_hash.0.into()),
        }
    }
}

impl TryFrom<protobuf::DeclaredClass> for DeprecatedDeclaredClass {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::DeclaredClass) -> Result<Self, Self::Error> {
        Ok(DeprecatedDeclaredClass {
            class_hash: ClassHash(
                value
                    .class_hash
                    .ok_or(ProtobufConversionError::MissingField {
                        field_description: "DeclaredClass::class_hash",
                    })?
                    .try_into()?,
            ),
        })
    }
}

impl From<DeprecatedDeclaredClass> for protobuf::DeclaredClass {
    fn from(value: DeprecatedDeclaredClass) -> Self {
        protobuf::DeclaredClass {
            class_hash: Some(value.class_hash.0.into()),
            compiled_class_hash: None,
        }
    }
}
