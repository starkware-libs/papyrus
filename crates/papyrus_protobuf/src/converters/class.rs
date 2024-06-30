use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};

use papyrus_common::pending_classes::ApiContractClass;
use prost::Message;
use starknet_api::core::EntryPointSelector;
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::{deprecated_contract_class, state};
use starknet_types_core::felt::Felt;

use super::common::volition_domain_to_enum_int;
use super::ProtobufConversionError;
use crate::sync::{ClassQuery, DataOrFin, Query};
use crate::{auto_impl_into_and_try_from_vec_u8, protobuf};

pub const DOMAIN: DataAvailabilityMode = DataAvailabilityMode::L1;

impl TryFrom<protobuf::ClassesResponse> for DataOrFin<ApiContractClass> {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ClassesResponse) -> Result<Self, Self::Error> {
        match value.class_message {
            Some(protobuf::classes_response::ClassMessage::Class(class)) => {
                Ok(Self(Some(class.try_into()?)))
            }
            Some(protobuf::classes_response::ClassMessage::Fin(_)) => Ok(Self(None)),
            None => Err(ProtobufConversionError::MissingField {
                field_description: "ClassesResponse::class_message",
            }),
        }
    }
}
impl From<DataOrFin<ApiContractClass>> for protobuf::ClassesResponse {
    fn from(value: DataOrFin<ApiContractClass>) -> Self {
        match value.0 {
            Some(class) => protobuf::ClassesResponse {
                class_message: Some(protobuf::classes_response::ClassMessage::Class(class.into())),
            },
            None => protobuf::ClassesResponse {
                class_message: Some(protobuf::classes_response::ClassMessage::Fin(
                    protobuf::Fin {},
                )),
            },
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(DataOrFin<ApiContractClass>, protobuf::ClassesResponse);

impl TryFrom<protobuf::Class> for ApiContractClass {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Class) -> Result<Self, Self::Error> {
        let class = match value.class {
            Some(protobuf::class::Class::Cairo0(class)) => {
                ApiContractClass::DeprecatedContractClass(
                    deprecated_contract_class::ContractClass::try_from(class)?,
                )
            }
            Some(protobuf::class::Class::Cairo1(class)) => {
                ApiContractClass::ContractClass(state::ContractClass::try_from(class)?)
            }
            None => {
                return Err(ProtobufConversionError::MissingField {
                    field_description: "Class::class",
                });
            }
        };
        Ok(class)
    }
}

impl From<ApiContractClass> for protobuf::Class {
    fn from(value: ApiContractClass) -> Self {
        let domain = u32::try_from(volition_domain_to_enum_int(DOMAIN))
            .expect("volition_domain_to_enum_int output should be convertible to u32");
        let class = match value {
            ApiContractClass::DeprecatedContractClass(class) => {
                protobuf::class::Class::Cairo0(class.into())
            }
            ApiContractClass::ContractClass(class) => protobuf::class::Class::Cairo1(class.into()),
        };
        protobuf::Class { domain, class: Some(class) }
    }
}

impl TryFrom<protobuf::Cairo0Class> for deprecated_contract_class::ContractClass {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Cairo0Class) -> Result<Self, Self::Error> {
        let mut entry_points_by_type = HashMap::new();

        if !value.constructors.is_empty() {
            entry_points_by_type.insert(
                deprecated_contract_class::EntryPointType::Constructor,
                value
                    .constructors
                    .into_iter()
                    .map(|entry_point| entry_point.try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        if !value.externals.is_empty() {
            entry_points_by_type.insert(
                deprecated_contract_class::EntryPointType::External,
                value
                    .externals
                    .into_iter()
                    .map(|entry_point| entry_point.try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        if !value.l1_handlers.is_empty() {
            entry_points_by_type.insert(
                deprecated_contract_class::EntryPointType::L1Handler,
                value
                    .l1_handlers
                    .into_iter()
                    .map(|entry_point| entry_point.try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        // TODO: fill abi
        let abi = None;
        // TODO: fill program
        let program = deprecated_contract_class::Program::default();

        Ok(Self { program, entry_points_by_type, abi })
    }
}

impl From<deprecated_contract_class::ContractClass> for protobuf::Cairo0Class {
    fn from(value: deprecated_contract_class::ContractClass) -> Self {
        protobuf::Cairo0Class {
            constructors: value
                .entry_points_by_type
                .get(&deprecated_contract_class::EntryPointType::Constructor)
                .unwrap_or(&vec![])
                .iter()
                .cloned()
                .map(protobuf::EntryPoint::from)
                .collect(),
            externals: value
                .entry_points_by_type
                .get(&deprecated_contract_class::EntryPointType::External)
                .unwrap_or(&vec![])
                .iter()
                .cloned()
                .map(protobuf::EntryPoint::from)
                .collect(),
            l1_handlers: value
                .entry_points_by_type
                .get(&deprecated_contract_class::EntryPointType::L1Handler)
                .unwrap_or(&vec![])
                .iter()
                .cloned()
                .map(protobuf::EntryPoint::from)
                .collect(),
            // TODO: fill abi and program
            abi: "".to_string(),
            program: "".to_string(),
        }
    }
}

impl TryFrom<protobuf::Cairo1Class> for state::ContractClass {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Cairo1Class) -> Result<Self, Self::Error> {
        let abi = value.abi;

        let sierra_program =
            value.program.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?;

        let mut entry_points_by_type = HashMap::new();
        let entry_points =
            value.entry_points.clone().ok_or(ProtobufConversionError::MissingField {
                field_description: "Cairo1Class::entry_points",
            })?;
        if !entry_points.constructors.is_empty() {
            entry_points_by_type.insert(
                state::EntryPointType::Constructor,
                entry_points
                    .constructors
                    .into_iter()
                    .map(|entry_point| entry_point.try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        if !entry_points.externals.is_empty() {
            entry_points_by_type.insert(
                state::EntryPointType::External,
                entry_points
                    .externals
                    .into_iter()
                    .map(|entry_point| entry_point.try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        if !entry_points.l1_handlers.is_empty() {
            entry_points_by_type.insert(
                state::EntryPointType::L1Handler,
                entry_points
                    .l1_handlers
                    .into_iter()
                    .map(|entry_point| entry_point.try_into())
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }

        Ok(state::ContractClass { sierra_program, entry_points_by_type, abi })
    }
}

impl From<state::ContractClass> for protobuf::Cairo1Class {
    fn from(value: state::ContractClass) -> Self {
        let abi = value.abi;

        let program =
            value.sierra_program.clone().into_iter().map(protobuf::Felt252::from).collect();

        let entry_points = Some(protobuf::Cairo1EntryPoints {
            constructors: value
                .entry_points_by_type
                .get(&state::EntryPointType::Constructor)
                .unwrap_or(&vec![])
                .iter()
                .cloned()
                .map(protobuf::SierraEntryPoint::from)
                .collect(),

            externals: value
                .entry_points_by_type
                .get(&state::EntryPointType::External)
                .unwrap_or(&vec![])
                .iter()
                .cloned()
                .map(protobuf::SierraEntryPoint::from)
                .collect(),
            l1_handlers: value
                .entry_points_by_type
                .get(&state::EntryPointType::L1Handler)
                .unwrap_or(&vec![])
                .iter()
                .cloned()
                .map(protobuf::SierraEntryPoint::from)
                .collect(),
        });

        let contract_class_version = format!(
            "sierra-v{}.{}.{} cairo-v{}.{}.{}",
            value.sierra_program[0],
            value.sierra_program[1],
            value.sierra_program[2],
            value.sierra_program[3],
            value.sierra_program[4],
            value.sierra_program[5]
        );

        protobuf::Cairo1Class { abi, program, entry_points, contract_class_version }
    }
}

impl TryFrom<protobuf::EntryPoint> for deprecated_contract_class::EntryPoint {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::EntryPoint) -> Result<Self, Self::Error> {
        let selector_felt =
            Felt::try_from(value.selector.ok_or(ProtobufConversionError::MissingField {
                field_description: "EntryPoint::selector",
            })?)?;
        let selector = EntryPointSelector(selector_felt);

        let offset = deprecated_contract_class::EntryPointOffset(
            value.offset.try_into().expect("Failed converting u64 to usize"),
        );

        Ok(deprecated_contract_class::EntryPoint { selector, offset })
    }
}

impl From<deprecated_contract_class::EntryPoint> for protobuf::EntryPoint {
    fn from(value: deprecated_contract_class::EntryPoint) -> Self {
        protobuf::EntryPoint {
            selector: Some(value.selector.0.into()),
            offset: u64::try_from(value.offset.0).expect("Failed converting usize to u64"),
        }
    }
}

impl TryFrom<protobuf::SierraEntryPoint> for state::EntryPoint {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::SierraEntryPoint) -> Result<Self, Self::Error> {
        let selector_felt =
            Felt::try_from(value.selector.ok_or(ProtobufConversionError::MissingField {
                field_description: "SierraEntryPoint::selector",
            })?)?;
        let selector = EntryPointSelector(selector_felt);

        let function_idx =
            state::FunctionIndex(value.index.try_into().expect("Failed converting u64 to usize"));

        Ok(state::EntryPoint { function_idx, selector })
    }
}

impl From<state::EntryPoint> for protobuf::SierraEntryPoint {
    fn from(value: state::EntryPoint) -> Self {
        protobuf::SierraEntryPoint {
            index: u64::try_from(value.function_idx.0).expect("Failed converting usize to u64"),
            selector: Some(value.selector.0.into()),
        }
    }
}

impl TryFrom<protobuf::ClassesRequest> for Query {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ClassesRequest) -> Result<Self, Self::Error> {
        Ok(ClassQuery::try_from(value)?.0)
    }
}

impl TryFrom<protobuf::ClassesRequest> for ClassQuery {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ClassesRequest) -> Result<Self, Self::Error> {
        Ok(ClassQuery(
            value
                .iteration
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "ClassesRequest::iteration",
                })?
                .try_into()?,
        ))
    }
}

impl From<Query> for protobuf::ClassesRequest {
    fn from(value: Query) -> Self {
        protobuf::ClassesRequest { iteration: Some(value.into()) }
    }
}

impl From<ClassQuery> for protobuf::ClassesRequest {
    fn from(value: ClassQuery) -> Self {
        protobuf::ClassesRequest { iteration: Some(value.0.into()) }
    }
}

auto_impl_into_and_try_from_vec_u8!(ClassQuery, protobuf::ClassesRequest);
