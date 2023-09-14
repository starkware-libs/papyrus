use futures::{AsyncRead, AsyncWrite, StreamExt};
use libp2p::core::multiaddr::multiaddr;
use libp2p::core::transport::memory::MemoryTransport;
use libp2p::core::transport::{ListenerId, Transport};

pub(crate) async fn get_connected_streams()
-> (impl AsyncRead + AsyncWrite, impl AsyncRead + AsyncWrite) {
    let address = multiaddr![Memory(0u64)];
    let mut transport = MemoryTransport::new().boxed();
    transport.listen_on(ListenerId::next(), address).unwrap();
    let listener_addr = transport
        .select_next_some()
        .await
        .into_new_address()
        .expect("MemoryTransport not listening on an address!");

    tokio::join!(
        async move {
            let transport_event = transport.next().await.unwrap();
            let (listener_upgrade, _) = transport_event.into_incoming().unwrap();
            listener_upgrade.await.unwrap()
        },
        async move { MemoryTransport::new().dial(listener_addr).unwrap().await.unwrap() },
    )
}
