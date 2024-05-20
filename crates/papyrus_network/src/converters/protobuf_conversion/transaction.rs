use std::convert::{TryFrom, TryInto};

use starknet_api::core::{ClassHash, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::transaction::{
    Calldata,
    ContractAddressSalt,
    DeployAccountTransactionV1,
    Fee,
    TransactionSignature,
};

use super::ProtobufConversionError;
use crate::protobuf_messages::protobuf::{self};

// TODO: use the conversion in Starknet api once its upgraded
fn try_from_starkfelt_to_u128(felt: StarkFelt) -> Result<u128, &'static str> {
    const COMPLIMENT_OF_U128: usize = 16; // 32 - 16
    let (rest, u128_bytes) = felt.bytes().split_at(COMPLIMENT_OF_U128);
    if rest != [0u8; COMPLIMENT_OF_U128] {
        return Err("Value out of range");
    }

    let bytes: [u8; 16] = match u128_bytes.try_into() {
        Ok(b) => b,
        Err(_) => return Err("Failed to convert bytes to u128"),
    };

    Ok(u128::from_be_bytes(bytes))
}

impl TryFrom<protobuf::transaction::DeployAccountV1> for DeployAccountTransactionV1 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::DeployAccountV1) -> Result<Self, Self::Error> {
        let max_fee_felt =
            StarkFelt::try_from(value.max_fee.ok_or(ProtobufConversionError::MissingField {
                field_description: "DeployAccountV1::max_fee",
            })?)?;
        let max_fee = Fee(try_from_starkfelt_to_u128(max_fee_felt).map_err(|_| {
            ProtobufConversionError::OutOfRangeValue {
                type_description: "u128",
                value_as_str: format!("{max_fee_felt:?}"),
            }
        })?);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV1::signature",
                })?
                .parts
                .into_iter()
                .map(StarkFelt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        let nonce = Nonce(
            value
                .nonce
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV1::nonce",
                })?
                .try_into()?,
        );

        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV1::class_hash",
                })?
                .try_into()?,
        );

        let contract_address_salt = ContractAddressSalt(
            value
                .address_salt
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV1::address_salt",
                })?
                .try_into()?,
        );

        let constructor_calldata =
            value.calldata.into_iter().map(StarkFelt::try_from).collect::<Result<Vec<_>, _>>()?;

        let constructor_calldata = Calldata(constructor_calldata.into());

        Ok(Self {
            max_fee,
            signature,
            nonce,
            class_hash,
            contract_address_salt,
            constructor_calldata,
        })
    }
}
