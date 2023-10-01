pub mod block;
pub mod common;
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

use futures::{AsyncRead, AsyncWrite};
use libp2p::core::upgrade::{read_length_prefixed, write_length_prefixed};
use prost::Message;

pub use crate::messages::proto::p2p::proto as protobuf;

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
) -> Result<Option<T>, io::Error> {
    let buf = read_length_prefixed(&mut io, MAX_MESSAGE_SIZE).await?;
    // read_length_prefixed returns an empty vec if it reaches EOF. We opened an issue in libp2p to
    // try and change this: https://github.com/libp2p/rust-libp2p/issues/4565
    // TODO(shahak): This currently disables reading empty messages. fix this by copying the
    // code from libp2p and changing it.
    if buf.is_empty() {
        return Ok(None);
    }
    Ok(Some(T::decode(buf.as_slice())?))
}
