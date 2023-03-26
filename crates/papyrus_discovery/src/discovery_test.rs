use libp2p::core::transport::dummy::DummyTransport;
use libp2p::swarm::Swarm;
use libp2p::PeerId;

#[test]
fn basic_usage() {
    let peer_id0 = PeerId::random();
    let peer_id1 = PeerId::random();
    let swarm0 = Swarm::with_async_std_executor(
        DummyTransport::new.boxed(),
        DiscoveryBehaviour::new(peer_id1),
        peer_id0,
    );
    let swarm1 = Swarm::with_async_std_executor(
        DummyTransport::new.boxed(),
        DiscoveryBehaviour::new(peer_id1),
        peer_id0,
    );
    loop {
        swarm0.next();
        swarm1.next();
    }
}
