use std::convert::{TryFrom, TryInto};

use starknet_api::core::{ClassHash, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::transaction::{
    Calldata,
    ContractAddressSalt,
    DeployAccountTransactionV1,
    DeployAccountTransactionV3,
    Fee,
    PaymasterData,
    Resource,
    ResourceBounds,
    ResourceBoundsMapping,
    Tip,
    TransactionSignature,
};

use super::common::enum_int_to_volition_domain;
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

impl From<DeployAccountTransactionV1> for protobuf::transaction::DeployAccountV1 {
    fn from(value: DeployAccountTransactionV1) -> Self {
        Self {
            max_fee: Some(StarkFelt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|stark_felt| stark_felt.into()).collect(),
            }),
            nonce: Some(value.nonce.0.into()),
            class_hash: Some(value.class_hash.0.into()),
            address_salt: Some(value.contract_address_salt.0.into()),
            calldata: value
                .constructor_calldata
                .0
                .iter()
                .map(|calldata| (*calldata).into())
                .collect(),
        }
    }
}

impl TryFrom<protobuf::transaction::DeployAccountV3> for DeployAccountTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::DeployAccountV3) -> Result<Self, Self::Error> {
        let resource_bounds = ResourceBoundsMapping::try_from(value.resource_bounds.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "DeployAccountV3::resource_bounds",
            },
        )?)?;

        let tip = Tip(value.tip);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV3::signature",
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
                    field_description: "DeployAccountV3::nonce",
                })?
                .try_into()?,
        );

        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV3::class_hash",
                })?
                .try_into()?,
        );

        let contract_address_salt = ContractAddressSalt(
            value
                .address_salt
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeployAccountV3::address_salt",
                })?
                .try_into()?,
        );

        let constructor_calldata =
            value.calldata.into_iter().map(StarkFelt::try_from).collect::<Result<Vec<_>, _>>()?;

        let constructor_calldata = Calldata(constructor_calldata.into());

        let nonce_data_availability_mode =
            enum_int_to_volition_domain(value.nonce_data_availability_mode)?;

        let fee_data_availability_mode =
            enum_int_to_volition_domain(value.fee_data_availability_mode)?;

        let paymaster_data = PaymasterData(
            value
                .paymaster_data
                .into_iter()
                .map(StarkFelt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        Ok(Self {
            resource_bounds,
            tip,
            signature,
            nonce,
            class_hash,
            contract_address_salt,
            constructor_calldata,
            nonce_data_availability_mode,
            fee_data_availability_mode,
            paymaster_data,
        })
    }
}

impl TryFrom<protobuf::ResourceBounds> for ResourceBoundsMapping {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ResourceBounds) -> Result<Self, Self::Error> {
        let mut resource_bounds = ResourceBoundsMapping::default();
        if let Some(l1_gas) = value.l1_gas {
            let max_amount_felt = StarkFelt::try_from(l1_gas.max_amount.ok_or(
                ProtobufConversionError::MissingField {
                    field_description: "ResourceBounds::l1_gas::max_amount",
                },
            )?)?;
            let max_amount = max_amount_felt.try_into().map_err(|_| {
                ProtobufConversionError::OutOfRangeValue {
                    type_description: "Felt252",
                    value_as_str: format!("{max_amount_felt:?}"),
                }
            })?;

            let max_price_per_unit_felt = StarkFelt::try_from(l1_gas.max_price_per_unit.ok_or(
                ProtobufConversionError::MissingField {
                    field_description: "ResourceBounds::l1_gas::max_price_per_unit",
                },
            )?)?;
            let max_price_per_unit =
                try_from_starkfelt_to_u128(max_price_per_unit_felt).map_err(|_| {
                    ProtobufConversionError::OutOfRangeValue {
                        type_description: "u128",
                        value_as_str: format!("{max_price_per_unit_felt:?}"),
                    }
                })?;

            resource_bounds
                .0
                .insert(Resource::L1Gas, ResourceBounds { max_amount, max_price_per_unit });
        }
        if let Some(l2_gas) = value.l2_gas {
            let max_amount_felt = StarkFelt::try_from(l2_gas.max_amount.ok_or(
                ProtobufConversionError::MissingField {
                    field_description: "ResourceBounds::l2_gas::max_amount",
                },
            )?)?;
            let max_amount = max_amount_felt.try_into().map_err(|_| {
                ProtobufConversionError::OutOfRangeValue {
                    type_description: "Felt252",
                    value_as_str: format!("{max_amount_felt:?}"),
                }
            })?;

            let max_price_per_unit_felt = StarkFelt::try_from(l2_gas.max_price_per_unit.ok_or(
                ProtobufConversionError::MissingField {
                    field_description: "ResourceBounds::l2_gas::max_price_per_unit",
                },
            )?)?;
            let max_price_per_unit =
                try_from_starkfelt_to_u128(max_price_per_unit_felt).map_err(|_| {
                    ProtobufConversionError::OutOfRangeValue {
                        type_description: "u128",
                        value_as_str: format!("{max_price_per_unit_felt:?}"),
                    }
                })?;
            resource_bounds
                .0
                .insert(Resource::L2Gas, ResourceBounds { max_amount, max_price_per_unit });
        }
        Ok(resource_bounds)
    }
}

impl From<ResourceBoundsMapping> for protobuf::ResourceBounds {
    fn from(value: ResourceBoundsMapping) -> Self {
        let mut res = protobuf::ResourceBounds::default();
        for (resource, resource_bounds) in value.0 {
            match resource {
                Resource::L1Gas => {
                    let resource_limits = protobuf::ResourceLimits {
                        max_amount: Some(StarkFelt::from(resource_bounds.max_amount).into()),
                        max_price_per_unit: Some(
                            StarkFelt::from(resource_bounds.max_price_per_unit).into(),
                        ),
                    };
                    res.l1_gas = Some(resource_limits);
                }
                Resource::L2Gas => {
                    let resource_limits = protobuf::ResourceLimits {
                        max_amount: Some(StarkFelt::from(resource_bounds.max_amount).into()),
                        max_price_per_unit: Some(
                            StarkFelt::from(resource_bounds.max_price_per_unit).into(),
                        ),
                    };
                    res.l2_gas = Some(resource_limits);
                }
            }
        }
        res
    }
}
