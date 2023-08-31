pub mod block;
pub mod common;

pub mod proto {
    pub mod p2p {
        pub mod proto {
            include!(concat!(env!("OUT_DIR"), "/_.rs"));
        }
    }
}

use std::io;

use futures::{AsyncRead, AsyncWrite};
use libp2p::core::upgrade::{read_length_prefixed, write_length_prefixed};
use prost::Message;

pub const MAX_MESSAGE_SIZE: usize = 1 << 20;

pub async fn write_message<T: Message, Stream: AsyncWrite + Unpin>(
    message: T,
    mut io: Stream,
) -> Result<(), io::Error> {
    write_length_prefixed(&mut io, &message.encode_to_vec()).await?;
    Ok(())
}

pub async fn read_message<T: Message + Default, Stream: AsyncRead + Unpin>(
    mut io: Stream,
) -> Result<T, io::Error> {
    let buf = read_length_prefixed(&mut io, MAX_MESSAGE_SIZE).await?;
    Ok(T::decode(buf.as_slice())?)
}
