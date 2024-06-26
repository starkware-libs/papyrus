#[cfg(test)]
#[path = "event_test.rs"]
mod event_test;
use prost::Message;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::transaction::{Event, EventContent, EventData, EventKey, TransactionHash};
use starknet_types_core::felt::Felt;

use super::ProtobufConversionError;
use crate::sync::{DataOrFin, EventQuery, Query};
use crate::{auto_impl_into_and_try_from_vec_u8, protobuf};

impl TryFrom<protobuf::EventsResponse> for DataOrFin<(Event, TransactionHash)> {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::EventsResponse) -> Result<Self, Self::Error> {
        match value.event_message {
            Some(protobuf::events_response::EventMessage::Event(event)) => {
                Ok(Self(Some(event.try_into()?)))
            }
            Some(protobuf::events_response::EventMessage::Fin(_)) => Ok(Self(None)),
            None => Err(ProtobufConversionError::MissingField {
                field_description: "EventsResponse::event_message",
            }),
        }
    }
}
impl From<DataOrFin<(Event, TransactionHash)>> for protobuf::EventsResponse {
    fn from(value: DataOrFin<(Event, TransactionHash)>) -> Self {
        match value.0 {
            Some(event_transaction_hash) => protobuf::EventsResponse {
                event_message: Some(protobuf::events_response::EventMessage::Event(
                    event_transaction_hash.into(),
                )),
            },
            None => protobuf::EventsResponse {
                event_message: Some(protobuf::events_response::EventMessage::Fin(protobuf::Fin {})),
            },
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(DataOrFin<(Event, TransactionHash)>, protobuf::EventsResponse);

impl TryFrom<protobuf::Event> for (Event, TransactionHash) {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Event) -> Result<Self, Self::Error> {
        let transaction_hash = TransactionHash(
            value
                .transaction_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "Event::transaction_hash",
                })?
                .try_into()?,
        );

        let from_address_felt =
            Felt::try_from(value.from_address.ok_or(ProtobufConversionError::MissingField {
                field_description: "Event::from_address",
            })?)?;
        let from_address =
            ContractAddress(PatriciaKey::try_from(from_address_felt).map_err(|_| {
                ProtobufConversionError::OutOfRangeValue {
                    type_description: "PatriciaKey",
                    value_as_str: format!("{from_address_felt:?}"),
                }
            })?);

        let keys = value
            .keys
            .into_iter()
            .map(Felt::try_from)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(EventKey)
            .collect();

        let data =
            EventData(value.data.into_iter().map(Felt::try_from).collect::<Result<Vec<_>, _>>()?);

        Ok((Event { from_address, content: EventContent { keys, data } }, transaction_hash))
    }
}

impl From<(Event, TransactionHash)> for protobuf::Event {
    fn from(value: (Event, TransactionHash)) -> Self {
        let (event, transaction_hash) = value;
        let transaction_hash = Some(transaction_hash.0.into());
        let from_address = Some(Felt::from(event.from_address).into());
        let keys = event.content.keys.into_iter().map(|key| key.0.into()).collect();
        let data =
            event.content.data.0.into_iter().map(protobuf::Felt252::from).collect::<Vec<_>>();
        Self { transaction_hash, from_address, keys, data }
    }
}

impl TryFrom<protobuf::EventsRequest> for Query {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::EventsRequest) -> Result<Self, Self::Error> {
        Ok(EventQuery::try_from(value)?.0)
    }
}

impl TryFrom<protobuf::EventsRequest> for EventQuery {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::EventsRequest) -> Result<Self, Self::Error> {
        Ok(EventQuery(
            value
                .iteration
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "EventsRequest::iteration",
                })?
                .try_into()?,
        ))
    }
}

impl From<Query> for protobuf::EventsRequest {
    fn from(value: Query) -> Self {
        protobuf::EventsRequest { iteration: Some(value.into()) }
    }
}

impl From<EventQuery> for protobuf::EventsRequest {
    fn from(value: EventQuery) -> Self {
        protobuf::EventsRequest { iteration: Some(value.0.into()) }
    }
}

auto_impl_into_and_try_from_vec_u8!(EventQuery, protobuf::EventsRequest);
