#[cfg(test)]
mod messages_test;

pub mod protobuf {
    include!(concat!(env!("OUT_DIR"), "/_.rs"));
}

use std::io;

use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use prost::Message;
use prost_types::Timestamp;
use unsigned_varint::encode::usize_buffer;

pub const MAX_MESSAGE_SIZE: usize = 1 << 20;

pub async fn write_message<T: Message, Stream: AsyncWrite + Unpin>(
    message: T,
    mut io: Stream,
) -> Result<(), io::Error> {
    let message_vec = message.encode_to_vec();
    // This code is based on write_length_prefixed from libp2p v0.52 which was erased in v0.53.
    write_usize(&mut io, message_vec.len()).await?;
    io.write_all(&message_vec).await?;
    io.flush().await?;
    Ok(())
}

pub async fn read_message<T: Message + Default, Stream: AsyncRead + Unpin>(
    mut io: Stream,
) -> Result<Option<T>, io::Error> {
    // This code is based on read_length_prefixed from libp2p v0.52 which was erased in v0.53.
    let Some(message_len) = read_usize(&mut io).await? else { return Ok(None) };
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

// This code is based on read_varint from libp2p v0.52 which was erased in v0.53. The difference
// from there is that here we return None if we have EOF before starting to read.
pub async fn read_usize<Stream: AsyncRead + Unpin>(
    mut io: Stream,
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
    mut io: Stream,
    num: usize,
) -> Result<(), io::Error> {
    let mut buffer = usize_buffer();
    let encoded_len = unsigned_varint::encode::usize(num, &mut buffer).len();
    io.write_all(&buffer[..encoded_len]).await?;

    Ok(())
}

impl From<starknet_api::block::BlockHeader> for protobuf::BlockHeader {
    fn from(value: starknet_api::block::BlockHeader) -> Self {
        Self {
            parent_header: Some(protobuf::Hash { elements: value.parent_hash.0.bytes().to_vec() }),
            number: value.block_number.0,
            sequencer_address: Some(protobuf::Address {
                elements: value.sequencer.0.key().bytes().to_vec(),
            }),
            // TODO: fix timestamp conversion and add missing fields.
            time: Some(Timestamp { seconds: value.timestamp.0.try_into().unwrap_or(0), nanos: 0 }),
            state_diffs: None,
            state: None,
            proof_fact: None,
            transactions: None,
            events: None,
            receipts: None,
        }
    }
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

impl TryFrom<protobuf::Hash> for starknet_api::hash::StarkFelt {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::Hash) -> Result<Self, Self::Error> {
        let mut felt = [0; 32];
        if value.elements.len() != 32 {
            return Err(ProtobufConversionError::BytesDataLengthMismatch);
        }
        felt.copy_from_slice(&value.elements);
        if let Ok(stark_felt) = Self::new(felt) {
            Ok(stark_felt)
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
