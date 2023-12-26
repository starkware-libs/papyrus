#[cfg(test)]
mod messages_test;

pub mod proto {
    pub mod p2p {
        pub mod proto {
            include!(concat!(env!("OUT_DIR"), "/_.rs"));
        }
    }
}

use std::io;

use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use prost::Message;
use unsigned_varint::encode::usize_buffer;

pub use crate::messages::proto::p2p::proto as protobuf;

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
