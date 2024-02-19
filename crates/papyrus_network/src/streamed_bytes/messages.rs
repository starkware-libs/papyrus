#[cfg(test)]
#[path = "messages_test.rs"]
mod messages_test;

use std::io;

use futures::io::{ReadHalf, WriteHalf};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use unsigned_varint::encode::usize_buffer;

use super::Bytes;

pub const MAX_MESSAGE_SIZE: usize = 1 << 20;

pub async fn write_message<Stream: AsyncWrite + Unpin>(
    message: &Bytes,
    io: &mut Stream,
) -> Result<(), io::Error> {
    write_usize(io, message.len()).await?;
    io.write_all(message).await?;
    Ok(())
}

pub async fn write_message_without_length_prefix<Stream: AsyncWrite + Unpin>(
    message: &Bytes,
    // We require `io` to be WriteHalf<Stream> and not Stream in order to ensure it's not a
    // reference.
    // We want to ensure it's not a reference because this function will make it unusable
    mut io: WriteHalf<Stream>,
) -> Result<(), io::Error> {
    io.write_all(message).await?;
    io.close().await?;
    Ok(())
}

pub async fn read_message<Stream: AsyncRead + Unpin>(
    io: &mut Stream,
) -> Result<Option<Bytes>, io::Error> {
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
    let mut buf = vec![0u8; message_len];
    io.read_exact(&mut buf).await?;
    Ok(Some(buf))
}

pub async fn read_message_without_length_prefix<Stream: AsyncRead + Unpin>(
    // We require `io` to be ReadHalf<Stream> and not Stream in order to ensure it's not a
    // reference.
    // We want to ensure it's not a reference because this function will make it unusable
    mut io: ReadHalf<Stream>,
) -> Result<Bytes, io::Error> {
    let mut buf = vec![];
    io.read_to_end(&mut buf).await?;
    Ok(buf)
}

// This code is based on read_varint from libp2p v0.52 which was erased in v0.53. The difference
// from there is that here we return None if we have EOF before starting to read.
async fn read_usize<Stream: AsyncRead + Unpin>(
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
async fn write_usize<Stream: AsyncWrite + Unpin>(
    io: &mut Stream,
    num: usize,
) -> Result<(), io::Error> {
    let mut buffer = usize_buffer();
    let encoded_len = unsigned_varint::encode::usize(num, &mut buffer).len();
    io.write_all(&buffer[..encoded_len]).await?;

    Ok(())
}
