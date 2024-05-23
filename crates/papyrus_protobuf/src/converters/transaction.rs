use std::convert::{TryFrom, TryInto};

use starknet_api::core::{ClassHash, CompiledClassHash, EntryPointSelector, Nonce};
use starknet_api::hash::StarkFelt;
use starknet_api::transaction::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    DeclareTransactionV0V1,
    DeclareTransactionV2,
    DeclareTransactionV3,
    DeployAccountTransactionV1,
    DeployAccountTransactionV3,
    Fee,
    InvokeTransactionV0,
    InvokeTransactionV1,
    InvokeTransactionV3,
    PaymasterData,
    Resource,
    ResourceBounds,
    ResourceBoundsMapping,
    Tip,
    TransactionSignature,
};

use super::common::{enum_int_to_volition_domain, volition_domain_to_enum_int};
use super::ProtobufConversionError;
use crate::protobuf;

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

impl From<DeployAccountTransactionV3> for protobuf::transaction::DeployAccountV3 {
    fn from(value: DeployAccountTransactionV3) -> Self {
        Self {
            resource_bounds: Some(protobuf::ResourceBounds::from(value.resource_bounds)),
            tip: value.tip.0,
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
            nonce_data_availability_mode: volition_domain_to_enum_int(
                value.nonce_data_availability_mode,
            ),
            fee_data_availability_mode: volition_domain_to_enum_int(
                value.fee_data_availability_mode,
            ),
            paymaster_data: value
                .paymaster_data
                .0
                .iter()
                .map(|paymaster_data| (*paymaster_data).into())
                .collect(),
        }
    }
}

impl TryFrom<protobuf::ResourceBounds> for ResourceBoundsMapping {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ResourceBounds) -> Result<Self, Self::Error> {
        let mut resource_bounds = ResourceBoundsMapping::default();
        let Some(l1_gas) = value.l1_gas else {
            return Err(ProtobufConversionError::MissingField {
                field_description: "ResourceBounds::l1_gas",
            });
        };
        let max_amount_felt = StarkFelt::try_from(l1_gas.max_amount.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "ResourceBounds::l1_gas::max_amount",
            },
        )?)?;
        let max_amount =
            max_amount_felt.try_into().map_err(|_| ProtobufConversionError::OutOfRangeValue {
                type_description: "u64",
                value_as_str: format!("{max_amount_felt:?}"),
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
        let Some(l2_gas) = value.l2_gas else {
            return Err(ProtobufConversionError::MissingField {
                field_description: "ResourceBounds::l2_gas",
            });
        };
        let max_amount_felt = StarkFelt::try_from(l2_gas.max_amount.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "ResourceBounds::l2_gas::max_amount",
            },
        )?)?;
        let max_amount =
            max_amount_felt.try_into().map_err(|_| ProtobufConversionError::OutOfRangeValue {
                type_description: "u64",
                value_as_str: format!("{max_amount_felt:?}"),
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
        Ok(resource_bounds)
    }
}

impl From<ResourceBoundsMapping> for protobuf::ResourceBounds {
    fn from(value: ResourceBoundsMapping) -> Self {
        let mut res = protobuf::ResourceBounds::default();

        let resource_bounds_default = ResourceBounds::default();
        let resource_bounds_l1 = value.0.get(&Resource::L1Gas).unwrap_or(&resource_bounds_default);

        let resource_limits_l1 = protobuf::ResourceLimits {
            max_amount: Some(StarkFelt::from(resource_bounds_l1.max_amount).into()),
            max_price_per_unit: Some(StarkFelt::from(resource_bounds_l1.max_price_per_unit).into()),
        };
        res.l1_gas = Some(resource_limits_l1);

        let resource_bounds_default = ResourceBounds::default();
        let resource_bounds_l2 = value.0.get(&Resource::L2Gas).unwrap_or(&resource_bounds_default);

        let resource_limits_l2 = protobuf::ResourceLimits {
            max_amount: Some(StarkFelt::from(resource_bounds_l2.max_amount).into()),
            max_price_per_unit: Some(StarkFelt::from(resource_bounds_l2.max_price_per_unit).into()),
        };
        res.l2_gas = Some(resource_limits_l2);

        res
    }
}

impl TryFrom<protobuf::transaction::InvokeV0> for InvokeTransactionV0 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::InvokeV0) -> Result<Self, Self::Error> {
        let max_fee_felt =
            StarkFelt::try_from(value.max_fee.ok_or(ProtobufConversionError::MissingField {
                field_description: "InvokeV0::max_fee",
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
                    field_description: "InvokeV0::signature",
                })?
                .parts
                .into_iter()
                .map(StarkFelt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        let contract_address = value
            .address
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "InvokeV0::address",
            })?
            .try_into()?;

        let entry_point_selector_felt = StarkFelt::try_from(value.entry_point_selector.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "InvokeV0::entry_point_selector",
            },
        )?)?;
        let entry_point_selector = EntryPointSelector(entry_point_selector_felt);

        let calldata =
            value.calldata.into_iter().map(StarkFelt::try_from).collect::<Result<Vec<_>, _>>()?;

        let calldata = Calldata(calldata.into());

        Ok(Self { max_fee, signature, contract_address, entry_point_selector, calldata })
    }
}

impl From<InvokeTransactionV0> for protobuf::transaction::InvokeV0 {
    fn from(value: InvokeTransactionV0) -> Self {
        Self {
            max_fee: Some(StarkFelt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|stark_felt| stark_felt.into()).collect(),
            }),
            address: Some(value.contract_address.into()),
            entry_point_selector: Some(value.entry_point_selector.0.into()),
            calldata: value.calldata.0.iter().map(|calldata| (*calldata).into()).collect(),
        }
    }
}

impl TryFrom<protobuf::transaction::InvokeV1> for InvokeTransactionV1 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::InvokeV1) -> Result<Self, Self::Error> {
        let max_fee_felt =
            StarkFelt::try_from(value.max_fee.ok_or(ProtobufConversionError::MissingField {
                field_description: "InvokeV1::max_fee",
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
                    field_description: "InvokeV1::signature",
                })?
                .parts
                .into_iter()
                .map(StarkFelt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        let sender_address = value
            .sender
            .ok_or(ProtobufConversionError::MissingField { field_description: "InvokeV1::sender" })?
            .try_into()?;

        let nonce = Nonce(
            value
                .nonce
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "InvokeV1::nonce",
                })?
                .try_into()?,
        );

        let calldata =
            value.calldata.into_iter().map(StarkFelt::try_from).collect::<Result<Vec<_>, _>>()?;

        let calldata = Calldata(calldata.into());

        Ok(Self { max_fee, signature, nonce, sender_address, calldata })
    }
}

impl From<InvokeTransactionV1> for protobuf::transaction::InvokeV1 {
    fn from(value: InvokeTransactionV1) -> Self {
        Self {
            max_fee: Some(StarkFelt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|signature| signature.into()).collect(),
            }),
            sender: Some(value.sender_address.into()),
            nonce: Some(value.nonce.0.into()),
            calldata: value.calldata.0.iter().map(|calldata| (*calldata).into()).collect(),
        }
    }
}

impl TryFrom<protobuf::transaction::InvokeV3> for InvokeTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::InvokeV3) -> Result<Self, Self::Error> {
        let resource_bounds = ResourceBoundsMapping::try_from(value.resource_bounds.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "InvokeV3::resource_bounds",
            },
        )?)?;

        let tip = Tip(value.tip);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "InvokeV3::signature",
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
                    field_description: "InvokeV3::nonce",
                })?
                .try_into()?,
        );

        let sender_address = value
            .sender
            .ok_or(ProtobufConversionError::MissingField { field_description: "InvokeV3::sender" })?
            .try_into()?;

        let calldata =
            value.calldata.into_iter().map(StarkFelt::try_from).collect::<Result<Vec<_>, _>>()?;

        let calldata = Calldata(calldata.into());

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

        let account_deployment_data = AccountDeploymentData(
            value
                .account_deployment_data
                .into_iter()
                .map(StarkFelt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        Ok(Self {
            resource_bounds,
            tip,
            signature,
            nonce,
            sender_address,
            calldata,
            nonce_data_availability_mode,
            fee_data_availability_mode,
            paymaster_data,
            account_deployment_data,
        })
    }
}

impl From<InvokeTransactionV3> for protobuf::transaction::InvokeV3 {
    fn from(value: InvokeTransactionV3) -> Self {
        Self {
            resource_bounds: Some(protobuf::ResourceBounds::from(value.resource_bounds)),
            tip: value.tip.0,
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|signature| signature.into()).collect(),
            }),
            nonce: Some(value.nonce.0.into()),
            sender: Some(value.sender_address.into()),
            calldata: value.calldata.0.iter().map(|calldata| (*calldata).into()).collect(),
            nonce_data_availability_mode: volition_domain_to_enum_int(
                value.nonce_data_availability_mode,
            ),
            fee_data_availability_mode: volition_domain_to_enum_int(
                value.fee_data_availability_mode,
            ),
            paymaster_data: value
                .paymaster_data
                .0
                .iter()
                .map(|paymaster_data| (*paymaster_data).into())
                .collect(),
            account_deployment_data: value
                .account_deployment_data
                .0
                .iter()
                .map(|account_deployment_data| (*account_deployment_data).into())
                .collect(),
        }
    }
}

impl TryFrom<protobuf::transaction::DeclareV0> for DeclareTransactionV0V1 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::DeclareV0) -> Result<Self, Self::Error> {
        let max_fee_felt =
            StarkFelt::try_from(value.max_fee.ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV0::max_fee",
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
                    field_description: "DeclareV0::signature",
                })?
                .parts
                .into_iter()
                .map(StarkFelt::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        );

        // V0 transactions don't have a nonce, but the StarkNet API adds one to them
        let nonce = Nonce::default();

        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV0::class_hash",
                })?
                .try_into()?,
        );

        let sender_address = value
            .sender
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV0::sender",
            })?
            .try_into()?;

        Ok(Self { max_fee, signature, nonce, class_hash, sender_address })
    }
}

impl From<DeclareTransactionV0V1> for protobuf::transaction::DeclareV0 {
    fn from(value: DeclareTransactionV0V1) -> Self {
        Self {
            max_fee: Some(StarkFelt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|stark_felt| stark_felt.into()).collect(),
            }),
            sender: Some(value.sender_address.into()),
            class_hash: Some(value.class_hash.0.into()),
        }
    }
}

impl TryFrom<protobuf::transaction::DeclareV1> for DeclareTransactionV0V1 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::DeclareV1) -> Result<Self, Self::Error> {
        let max_fee_felt =
            StarkFelt::try_from(value.max_fee.ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV1::max_fee",
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
                    field_description: "DeclareV1::signature",
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
                    field_description: "DeclareV1::nonce",
                })?
                .try_into()?,
        );

        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV1::class_hash",
                })?
                .try_into()?,
        );

        let sender_address = value
            .sender
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV1::sender",
            })?
            .try_into()?;

        Ok(Self { max_fee, signature, nonce, class_hash, sender_address })
    }
}

impl From<DeclareTransactionV0V1> for protobuf::transaction::DeclareV1 {
    fn from(value: DeclareTransactionV0V1) -> Self {
        Self {
            max_fee: Some(StarkFelt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|stark_felt| stark_felt.into()).collect(),
            }),
            nonce: Some(value.nonce.0.into()),
            class_hash: Some(value.class_hash.0.into()),
            sender: Some(value.sender_address.into()),
        }
    }
}

impl TryFrom<protobuf::transaction::DeclareV2> for DeclareTransactionV2 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::DeclareV2) -> Result<Self, Self::Error> {
        let max_fee_felt =
            StarkFelt::try_from(value.max_fee.ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV2::max_fee",
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
                    field_description: "DeclareV2::signature",
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
                    field_description: "DeclareV2::nonce",
                })?
                .try_into()?,
        );

        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV2::class_hash",
                })?
                .try_into()?,
        );

        let compiled_class_hash = CompiledClassHash(
            value
                .compiled_class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV2::compiled_class_hash",
                })?
                .try_into()?,
        );

        let sender_address = value
            .sender
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV2::sender",
            })?
            .try_into()?;

        Ok(Self { max_fee, signature, nonce, class_hash, compiled_class_hash, sender_address })
    }
}

impl From<DeclareTransactionV2> for protobuf::transaction::DeclareV2 {
    fn from(value: DeclareTransactionV2) -> Self {
        Self {
            max_fee: Some(StarkFelt::from(value.max_fee.0).into()),
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|signature| signature.into()).collect(),
            }),
            nonce: Some(value.nonce.0.into()),
            class_hash: Some(value.class_hash.0.into()),
            compiled_class_hash: Some(value.compiled_class_hash.0.into()),
            sender: Some(value.sender_address.into()),
        }
    }
}

impl TryFrom<protobuf::transaction::DeclareV3> for DeclareTransactionV3 {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::transaction::DeclareV3) -> Result<Self, Self::Error> {
        let resource_bounds = ResourceBoundsMapping::try_from(value.resource_bounds.ok_or(
            ProtobufConversionError::MissingField {
                field_description: "DeclareV3::resource_bounds",
            },
        )?)?;

        let tip = Tip(value.tip);

        let signature = TransactionSignature(
            value
                .signature
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV3::signature",
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
                    field_description: "DeclareV3::nonce",
                })?
                .try_into()?,
        );

        let class_hash = ClassHash(
            value
                .class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV3::class_hash",
                })?
                .try_into()?,
        );

        let compiled_class_hash = CompiledClassHash(
            value
                .compiled_class_hash
                .ok_or(ProtobufConversionError::MissingField {
                    field_description: "DeclareV3::compiled_class_hash",
                })?
                .try_into()?,
        );

        let sender_address = value
            .sender
            .ok_or(ProtobufConversionError::MissingField {
                field_description: "DeclareV3::sender",
            })?
            .try_into()?;

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

        let account_deployment_data = AccountDeploymentData(
            value
                .account_deployment_data
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
            compiled_class_hash,
            sender_address,
            nonce_data_availability_mode,
            fee_data_availability_mode,
            paymaster_data,
            account_deployment_data,
        })
    }
}

impl From<DeclareTransactionV3> for protobuf::transaction::DeclareV3 {
    fn from(value: DeclareTransactionV3) -> Self {
        Self {
            resource_bounds: Some(protobuf::ResourceBounds::from(value.resource_bounds)),
            tip: value.tip.0,
            signature: Some(protobuf::AccountSignature {
                parts: value.signature.0.into_iter().map(|signature| signature.into()).collect(),
            }),
            nonce: Some(value.nonce.0.into()),
            class_hash: Some(value.class_hash.0.into()),
            compiled_class_hash: Some(value.compiled_class_hash.0.into()),
            sender: Some(value.sender_address.into()),
            nonce_data_availability_mode: volition_domain_to_enum_int(
                value.nonce_data_availability_mode,
            ),
            fee_data_availability_mode: volition_domain_to_enum_int(
                value.fee_data_availability_mode,
            ),
            paymaster_data: value
                .paymaster_data
                .0
                .iter()
                .map(|paymaster_data| (*paymaster_data).into())
                .collect(),
            account_deployment_data: value
                .account_deployment_data
                .0
                .iter()
                .map(|account_deployment_data| (*account_deployment_data).into())
                .collect(),
        }
    }
}
