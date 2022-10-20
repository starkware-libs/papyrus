use std::iter;

use futures::future;
use libp2p::core::UpgradeInfo;
use libp2p::{InboundUpgrade, OutboundUpgrade};

#[derive(Debug, Clone, Default)]
pub struct SyncProtocol;

impl UpgradeInfo for SyncProtocol {
    type Info = &'static [u8];
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(b"/starknet-p2p/sync/1.0.0")
    }
}

impl<C> InboundUpgrade<C> for SyncProtocol {
    type Output = C;
    type Error = UpgradeError;
    type Future = future::Ready<Result<Self::Output, UpgradeError>>;

    fn upgrade_inbound(self, socket: C, _: Self::Info) -> Self::Future {
        future::ok(socket)
    }
}

impl<C> OutboundUpgrade<C> for SyncProtocol
// where
// C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = C;
    type Error = UpgradeError;
    type Future = future::Ready<Result<Self::Output, UpgradeError>>;

    fn upgrade_outbound(self, socket: C, _: Self::Info) -> Self::Future {
        future::ok(socket)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UpgradeError {
    // #[error("Failed to encode or decode")]
    // Codec(
    //     #[from]
    //     #[source]
    //     prost_codec::Error,
    // ),
    // #[error("I/O interaction failed")]
    // Io(
    //     #[from]
    //     #[source]
    //     io::Error,
    // ),
    // #[error("Stream closed")]
    // StreamClosed,
    // #[error("Failed decoding multiaddr")]
    // Multiaddr(
    //     #[from]
    //     #[source]
    //     multiaddr::Error,
    // ),
    // #[error("Failed decoding public key")]
    // PublicKey(
    //     #[from]
    //     #[source]
    //     identity::error::DecodingError,
    // ),
}
