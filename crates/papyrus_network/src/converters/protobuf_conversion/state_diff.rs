use indexmap::IndexMap;
use starknet_api::core::{ClassHash, CompiledClassHash, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::state::{StorageKey, ThinStateDiff};

use super::common::iteration_to_query_parts;
use super::ProtobufConversionError;
use crate::protobuf_messages::protobuf;
use crate::{Direction, InternalQuery, Query};

impl TryFrom<protobuf::StateDiffsResponse> for Option<ThinStateDiff> {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::StateDiffsResponse) -> Result<Self, Self::Error> {
        match value.state_diff_message {
            Some(protobuf::state_diffs_response::StateDiffMessage::ContractDiff(contract_diff)) => {
                Ok(Some(contract_diff.try_into()?))
            }
            Some(protobuf::state_diffs_response::StateDiffMessage::DeclaredClass(
                declared_class,
            )) => Ok(Some(declared_class.try_into()?)),
            Some(protobuf::state_diffs_response::StateDiffMessage::Fin(_)) => Ok(None),
            None => Err(ProtobufConversionError::MissingField {
                field_description: "StateDiffsResponse::state_diff_message",
            }),
        }
    }
}

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
            .map(|hash| Ok(IndexMap::from_iter([(contract_address, ClassHash(hash.try_into()?))])))
            .transpose()?
            .unwrap_or_default();

        let storage_diffs = if value.values.is_empty() {
            IndexMap::new()
        } else {
            let storage_values = value
                .values
                .into_iter()
                .map(|stored_value| stored_value.try_into())
                .collect::<Result<IndexMap<StorageKey, StarkFelt>, _>>()?;
            IndexMap::from_iter([(contract_address, storage_values)])
        };

        let nonces = value
            .nonce
            .map(|nonce| Ok(IndexMap::from_iter([(contract_address, Nonce(nonce.try_into()?))])))
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

impl TryFrom<protobuf::ContractStoredValue> for (StorageKey, StarkFelt) {
    type Error = ProtobufConversionError;
    fn try_from(entry: protobuf::ContractStoredValue) -> Result<Self, Self::Error> {
        let key_felt =
            StarkFelt::try_from(entry.key.ok_or(ProtobufConversionError::MissingField {
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
        let value =
            StarkFelt::try_from(entry.value.ok_or(ProtobufConversionError::MissingField {
                field_description: "ContractStoredValue::value",
            })?)?;
        Ok((key, value))
    }
}

// A wrapper struct for Vec<StateDiffsResponse> so that we can implement traits for it.
pub struct StateDiffsResponseVec(pub Vec<protobuf::StateDiffsResponse>);

const DOMAIN: u32 = 0;

impl From<ThinStateDiff> for StateDiffsResponseVec {
    fn from(value: ThinStateDiff) -> Self {
        let mut result = Vec::new();

        for (contract_address, class_hash) in
            value.deployed_contracts.into_iter().chain(value.replaced_classes.into_iter())
        {
            result.push(protobuf::StateDiffsResponse {
                state_diff_message: Some(
                    protobuf::state_diffs_response::StateDiffMessage::ContractDiff(
                        protobuf::ContractDiff {
                            address: Some(contract_address.into()),
                            class_hash: Some(class_hash.0.into()),
                            domain: DOMAIN,
                            ..Default::default()
                        },
                    ),
                ),
            });
        }
        for (contract_address, storage_diffs) in value.storage_diffs {
            result.push(protobuf::StateDiffsResponse {
                state_diff_message: Some(
                    protobuf::state_diffs_response::StateDiffMessage::ContractDiff(
                        protobuf::ContractDiff {
                            address: Some(contract_address.into()),
                            values: storage_diffs
                                .into_iter()
                                .map(|(key, value)| protobuf::ContractStoredValue {
                                    key: Some((*key.0.key()).into()),
                                    value: Some(value.into()),
                                })
                                .collect(),
                            domain: DOMAIN,
                            ..Default::default()
                        },
                    ),
                ),
            });
        }
        for (contract_address, nonce) in value.nonces {
            result.push(protobuf::StateDiffsResponse {
                state_diff_message: Some(
                    protobuf::state_diffs_response::StateDiffMessage::ContractDiff(
                        protobuf::ContractDiff {
                            address: Some(contract_address.into()),
                            nonce: Some(nonce.0.into()),
                            domain: DOMAIN,
                            ..Default::default()
                        },
                    ),
                ),
            });
        }

        for (class_hash, compiled_class_hash) in value.declared_classes {
            result.push(protobuf::StateDiffsResponse {
                state_diff_message: Some(
                    protobuf::state_diffs_response::StateDiffMessage::DeclaredClass(
                        protobuf::DeclaredClass {
                            class_hash: Some(class_hash.0.into()),
                            compiled_class_hash: Some(compiled_class_hash.0.into()),
                        },
                    ),
                ),
            });
        }
        for class_hash in value.deprecated_declared_classes {
            result.push(protobuf::StateDiffsResponse {
                state_diff_message: Some(
                    protobuf::state_diffs_response::StateDiffMessage::DeclaredClass(
                        protobuf::DeclaredClass {
                            class_hash: Some(class_hash.0.into()),
                            compiled_class_hash: None,
                        },
                    ),
                ),
            });
        }

        Self(result)
    }
}

impl TryFrom<protobuf::StateDiffsRequest> for InternalQuery {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::StateDiffsRequest) -> Result<Self, Self::Error> {
        let value = value.iteration.ok_or(ProtobufConversionError::MissingField {
            field_description: "StateDiffsRequest::iteration",
        })?;
        let (start_block, direction, limit, step) = iteration_to_query_parts(value)?;
        Ok(Self { start_block, direction, limit, step })
    }
}

impl From<Query> for protobuf::StateDiffsRequest {
    fn from(value: Query) -> Self {
        protobuf::StateDiffsRequest {
            iteration: Some(protobuf::Iteration {
                direction: match value.direction {
                    Direction::Forward => 0,
                    Direction::Backward => 1,
                },
                limit: value.limit as u64,
                step: value.step as u64,
                start: Some(protobuf::iteration::Start::BlockNumber(value.start_block.0)),
            }),
        }
    }
}
