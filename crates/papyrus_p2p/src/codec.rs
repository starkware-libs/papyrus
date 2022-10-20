#[cfg(test)]
#[path = "codec_test.rs"]
mod codec_test;

pub mod common_proto {
    use rand::Rng;

    use super::ConversionError;

    include!(concat!(env!("OUT_DIR"), "/common_proto.rs"));

    impl From<starknet_api::BlockNumber> for BlockNumber {
        fn from(block_number: starknet_api::BlockNumber) -> Self {
            BlockNumber { number: block_number.number().to_owned() }
        }
    }

    impl From<BlockNumber> for starknet_api::BlockNumber {
        fn from(block_number: BlockNumber) -> Self {
            starknet_api::BlockNumber::new(block_number.number)
        }
    }

    impl From<&starknet_api::StarkHash> for FieldElement {
        fn from(hash: &starknet_api::StarkHash) -> Self {
            FieldElement { element: hash.bytes().to_vec() }
        }
    }

    impl From<starknet_api::BlockHeader> for BlockHeader {
        fn from(block_header: starknet_api::BlockHeader) -> Self {
            BlockHeader {
                parent_block_hash: Some(block_header.parent_hash.block_hash().into()),
                block_number: block_header.block_number.number().to_owned(),
                global_state_root: Some(block_header.state_root.root().into()),
                sequencer_address: Some(block_header.sequencer.contract_address().key().into()),
                block_timestamp: block_header.timestamp.time_stamp().to_owned(),
            }
        }
    }

    impl TryFrom<FieldElement> for starknet_api::StarkHash {
        type Error = ConversionError;

        fn try_from(felt: FieldElement) -> Result<Self, Self::Error> {
            Ok(starknet_api::StarkHash::new(
                felt.element.try_into().map_err(|_| ConversionError::BadFeltLength)?,
            )?)
        }
    }

    impl TryFrom<BlockHeader> for starknet_api::BlockHeader {
        type Error = ConversionError;

        fn try_from(block_header: BlockHeader) -> Result<Self, Self::Error> {
            let mut block_hash = rand::thread_rng().gen::<[u8; 32]>();
            block_hash[0] = 0;
            Ok(starknet_api::BlockHeader {
                block_hash: starknet_api::BlockHash::new(
                    FieldElement { element: block_hash.to_vec() }.try_into()?,
                ),
                parent_hash: starknet_api::BlockHash::new(
                    block_header
                        .parent_block_hash
                        .ok_or(ConversionError::MissingParentBlockHash)?
                        .try_into()?,
                ),
                block_number: starknet_api::BlockNumber::new(block_header.block_number.into()),
                gas_price: starknet_api::GasPrice::new(128),
                state_root: starknet_api::GlobalRoot::new(
                    block_header
                        .global_state_root
                        .ok_or(ConversionError::MissingGlobalRoot)?
                        .try_into()?,
                ),
                sequencer: starknet_api::StarkHash::try_from(
                    block_header
                        .sequencer_address
                        .ok_or(ConversionError::MissingSequencerAddress)?,
                )?
                .try_into()?,
                timestamp: starknet_api::BlockTimestamp::new(block_header.block_timestamp),
            })
        }
    }
}
pub mod sync_proto {
    include!(concat!(env!("OUT_DIR"), "/sync_proto.rs"));
}

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("Missing parent block hash field")]
    MissingParentBlockHash,
    #[error("Missing global root field")]
    MissingGlobalRoot,
    #[error("Missing sequencer address field")]
    MissingSequencerAddress,
    #[error("The requested PoW difficulty is out of range")]
    PoWDifficultyOutOfRange,
    #[error("The provided PoW hash is not 32 bytes long")]
    BadPoWHash,
    #[error("Out of range {string}.")]
    OutOfRange { string: String },
    #[error("The provided hash is not 32 bytes long")]
    BadFeltLength,
    #[error("Failed to decode felt")]
    BadFelt(#[from] starknet_api::StarknetApiError),
}
