#[cfg(test)]
mod messages_test;

pub mod protobuf {
    include!(concat!(env!("OUT_DIR"), "/_.rs"));
}

use std::io;

use futures::io::{ReadHalf, WriteHalf};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use prost::Message;
use starknet_api::data_availability::L1DataAvailabilityMode;
use unsigned_varint::encode::usize_buffer;

pub const MAX_MESSAGE_SIZE: usize = 1 << 20;
pub const PATRICIA_HEIGHT: u32 = 251;

pub async fn write_message<T: Message, Stream: AsyncWrite + Unpin>(
    message: T,
    io: &mut Stream,
) -> Result<(), io::Error> {
    let message_vec = message.encode_to_vec();
    // This code is based on write_length_prefixed from libp2p v0.52 which was erased in v0.53.
    write_usize(io, message_vec.len()).await?;
    io.write_all(&message_vec).await?;
    Ok(())
}

pub async fn write_message_without_length_prefix<T: Message, Stream: AsyncWrite + Unpin>(
    message: T,
    // We require `io` to be WriteHalf<Stream> and not Stream in order to ensure it's not a
    // reference.
    // We want to ensure it's not a reference because this function will make it unusable
    mut io: WriteHalf<Stream>,
) -> Result<(), io::Error> {
    let message_vec = message.encode_to_vec();
    io.write_all(&message_vec).await?;
    io.close().await?;
    Ok(())
}

pub async fn read_message<T: Message + Default, Stream: AsyncRead + Unpin>(
    io: &mut Stream,
) -> Result<Option<T>, io::Error> {
    // This code is based on read_length_prefixed from libp2p v0.52 which was erased in v0.53.
    let Some(message_len) = read_usize(io).await? else { return Ok(None) };
    if message_len > MAX_MESSAGE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Received data size ({message_len} bytes) exceeds maximum ({MAX_MESSAGE_SIZE} \
                 bytes)"
            ),
        ));
    }
    let mut buf = vec![0; message_len];
    io.read_exact(&mut buf).await?;
    Ok(Some(T::decode(buf.as_slice())?))
}

pub async fn read_message_without_length_prefix<T: Message + Default, Stream: AsyncRead + Unpin>(
    // We require `io` to be ReadHalf<Stream> and not Stream in order to ensure it's not a
    // reference.
    // We want to ensure it's not a reference because this function will make it unusable
    mut io: ReadHalf<Stream>,
) -> Result<T, io::Error> {
    let mut buf = vec![];
    io.read_to_end(&mut buf).await?;
    Ok(T::decode(buf.as_slice())?)
}

// This code is based on read_varint from libp2p v0.52 which was erased in v0.53. The difference
// from there is that here we return None if we have EOF before starting to read.
pub async fn read_usize<Stream: AsyncRead + Unpin>(
    io: &mut Stream,
) -> Result<Option<usize>, io::Error> {
    let mut buffer = unsigned_varint::encode::usize_buffer();
    let mut buffer_len = 0;

    loop {
        match io.read(&mut buffer[buffer_len..buffer_len + 1]).await? {
            0 => {
                // Reaching EOF before finishing to read the length is an error, unless the EOF is
                // at the very beginning of the substream, in which case we return None.
                if buffer_len == 0 {
                    return Ok(None);
                } else {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
            }
            n => debug_assert_eq!(n, 1),
        }

        buffer_len += 1;

        match unsigned_varint::decode::usize(&buffer[..buffer_len]) {
            Ok((len, _)) => return Ok(Some(len)),
            Err(unsigned_varint::decode::Error::Insufficient) => {}
            Err(error) => {
                return Err(io::Error::new(io::ErrorKind::InvalidData, error));
            }
        }
    }
}

// This code is based on write_varint from libp2p v0.52 which was erased in v0.53.
pub async fn write_usize<Stream: AsyncWrite + Unpin>(
    io: &mut Stream,
    num: usize,
) -> Result<(), io::Error> {
    let mut buffer = usize_buffer();
    let encoded_len = unsigned_varint::encode::usize(num, &mut buffer).len();
    io.write_all(&buffer[..encoded_len]).await?;

    Ok(())
}

#[derive(thiserror::Error, Debug)]
pub enum ProtobufConversionError {
    #[error("Out of range value")]
    OutOfRangeValue,
    #[error("Missing field")]
    MissingField,
    #[error("Bytes data length mismatch")]
    BytesDataLengthMismatch,
}

impl TryFrom<protobuf::Felt252> for starknet_api::hash::StarkFelt {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Felt252) -> Result<Self, Self::Error> {
        let mut felt = [0; 32];
        felt.copy_from_slice(&value.elements);
        if let Ok(stark_felt) = Self::new(felt) {
            Ok(stark_felt)
        } else {
            Err(ProtobufConversionError::OutOfRangeValue)
        }
    }
}

impl From<starknet_api::hash::StarkFelt> for protobuf::Felt252 {
    fn from(value: starknet_api::hash::StarkFelt) -> Self {
        Self { elements: value.bytes().to_vec() }
    }
}

impl From<starknet_api::block::BlockHash> for protobuf::Hash {
    fn from(value: starknet_api::block::BlockHash) -> Self {
        Self { elements: value.0.bytes().to_vec() }
    }
}

impl From<starknet_api::hash::StarkHash> for protobuf::Hash {
    fn from(value: starknet_api::hash::StarkHash) -> Self {
        Self { elements: value.bytes().to_vec() }
    }
}

impl From<starknet_api::core::ContractAddress> for protobuf::Address {
    fn from(value: starknet_api::core::ContractAddress) -> Self {
        Self { elements: value.0.key().bytes().to_vec() }
    }
}

impl From<u128> for protobuf::Uint128 {
    fn from(value: u128) -> Self {
        Self { high: (value >> 64) as u64, low: value as u64 }
    }
}

impl From<protobuf::Uint128> for u128 {
    fn from(value: protobuf::Uint128) -> Self {
        u128::from(value.low) + (u128::from(value.high) << 64)
    }
}

impl TryFrom<protobuf::Hash> for starknet_api::hash::StarkHash {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Hash) -> Result<Self, Self::Error> {
        let mut felt = [0; 32];
        if value.elements.len() != 32 {
            return Err(ProtobufConversionError::BytesDataLengthMismatch);
        }
        felt.copy_from_slice(&value.elements);
        if let Ok(stark_hash) = Self::new(felt) {
            Ok(stark_hash)
        } else {
            Err(ProtobufConversionError::OutOfRangeValue)
        }
    }
}

impl TryFrom<protobuf::Address> for starknet_api::core::ContractAddress {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Address) -> Result<Self, Self::Error> {
        let mut felt = [0; 32];
        if value.elements.len() != 32 {
            return Err(ProtobufConversionError::BytesDataLengthMismatch);
        }
        felt.copy_from_slice(&value.elements);
        if let Ok(hash) = starknet_api::hash::StarkHash::new(felt) {
            if let Ok(stark_felt) = starknet_api::core::PatriciaKey::try_from(hash) {
                Ok(starknet_api::core::ContractAddress(stark_felt))
            } else {
                Err(ProtobufConversionError::OutOfRangeValue)
            }
        } else {
            Err(ProtobufConversionError::OutOfRangeValue)
        }
    }
}

pub(crate) fn enum_int_to_l1_data_availability_mode(
    value: i32,
) -> Result<L1DataAvailabilityMode, ProtobufConversionError> {
    match value {
        0 => Ok(L1DataAvailabilityMode::Calldata),
        1 => Ok(L1DataAvailabilityMode::Blob),
        _ => Err(ProtobufConversionError::OutOfRangeValue),
    }
}

pub(crate) fn l1_data_availability_mode_to_enum_int(value: L1DataAvailabilityMode) -> i32 {
    match value {
        L1DataAvailabilityMode::Calldata => 0,
        L1DataAvailabilityMode::Blob => 1,
    }
}

#[cfg(test)]
pub(crate) trait TestInstance {
    fn test_instance() -> Self;
}

#[cfg(test)]
impl TestInstance for protobuf::Hash {
    fn test_instance() -> Self {
        Self { elements: [0].repeat(32).to_vec() }
    }
}

#[cfg(test)]
impl TestInstance for protobuf::Address {
    fn test_instance() -> Self {
        Self { elements: [0].repeat(32).to_vec() }
    }
}

#[cfg(test)]
impl TestInstance for protobuf::Patricia {
    fn test_instance() -> Self {
        Self { height: PATRICIA_HEIGHT, root: Some(protobuf::Hash::test_instance()) }
    }
}

#[cfg(test)]
impl TestInstance for protobuf::Merkle {
    fn test_instance() -> Self {
        Self { n_leaves: 0, root: Some(protobuf::Hash::test_instance()) }
    }
}

#[cfg(test)]
impl TestInstance for protobuf::Uint128 {
    fn test_instance() -> Self {
        Self { low: 1, high: 0 }
    }
}

#[cfg(test)]
impl TestInstance for protobuf::ConsensusSignature {
    fn test_instance() -> Self {
        Self {
            r: Some(protobuf::Felt252 { elements: [1].repeat(32).to_vec() }),
            s: Some(protobuf::Felt252 { elements: [1].repeat(32).to_vec() }),
        }
    }
}
