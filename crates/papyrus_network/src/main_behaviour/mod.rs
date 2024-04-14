pub(crate) mod mixed_behaviour;

use std::task::{ready, Context, Poll};

use libp2p::core::Endpoint;
use libp2p::identity::PublicKey;
use libp2p::kad::store::MemoryStore;
use libp2p::swarm::{
    ConnectionDenied,
    ConnectionHandler,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{identify, kad, multiaddr, Multiaddr, PeerId, StreamProtocol};
use mixed_behaviour::MixedBehaviour;

use self::mixed_behaviour::{BridgedBehaviour, Event as MixedBehaviourEvent};
use crate::peer_manager::{PeerManager, PeerManagerConfig};
use crate::{discovery, streamed_bytes, PeerAddressConfig};

// TODO(shahak): Make this an enum and fill its variants
struct Event;

// TODO(shahak): Find a better name for this.
struct MainBehaviour {
    mixed_behaviour: MixedBehaviour,
}

impl NetworkBehaviour for MainBehaviour {
    type ConnectionHandler = <MixedBehaviour as NetworkBehaviour>::ConnectionHandler;
    type ToSwarm = Event;

    // Required methods
    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        self.mixed_behaviour.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        self.mixed_behaviour.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
        )
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        self.mixed_behaviour.on_swarm_event(event)
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
        self.mixed_behaviour.on_connection_handler_event(peer_id, connection_id, event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, <Self::ConnectionHandler as ConnectionHandler>::FromBehaviour>>
    {
        let mixed_behaviour_event = ready!(self.mixed_behaviour.poll(cx));
        match mixed_behaviour_event {
            ToSwarm::GenerateEvent(MixedBehaviourEvent::InternalEvent(internal_event)) => {
                match internal_event {
                    mixed_behaviour::InternalEvent::NoOp => {}
                    mixed_behaviour::InternalEvent::NotifyKad(_) => {
                        self.mixed_behaviour.kademlia.on_other_behaviour_event(internal_event)
                    }
                    mixed_behaviour::InternalEvent::NotifyDiscovery(_) => {
                        self.mixed_behaviour.discovery.on_other_behaviour_event(internal_event)
                    }
                    mixed_behaviour::InternalEvent::NotifyStreamedBytes(_) => {
                        self.mixed_behaviour.streamed_bytes.on_other_behaviour_event(internal_event)
                    }
                }
                Poll::Pending
            }
            _ => Poll::Ready(mixed_behaviour_event.map_out(|_| Event)),
        }
    }
}

// TODO(shahak): If we decide to remove MainBehaviour, move this to MixedBehaviour.
impl MainBehaviour {
    // TODO(shahak): Consider adding Kademlia and Identify config to NetworkConfig.
    // TODO(shahak): Change PeerId in network config to KeyPair.
    // TODO(shahak): Add chain_id to NetworkConfig.
    // TODO(shahak): Add PeerManagerConfig to NetworkConfig.
    // TODO(shahak): remove allow dead code.
    #[allow(dead_code)]
    pub fn new(
        streamed_bytes_config: streamed_bytes::Config,
        public_key: PublicKey,
        chain_id: String,
        bootstrap_peer_config: PeerAddressConfig,
        peer_manager_config: PeerManagerConfig,
    ) -> Self {
        let peer_id = PeerId::from_public_key(&public_key);

        let mut kad_config = kad::Config::default();
        kad_config.set_protocol_names(vec![
            StreamProtocol::try_from_owned(format!("/starknet/kad/{chain_id}/1.0.0")).expect(
                "Strings that start with / should be converted successfully to StreamProtocol",
            ),
        ]);

        Self {
            mixed_behaviour: MixedBehaviour {
                peer_manager: PeerManager::new(peer_manager_config),
                discovery: discovery::Behaviour::new(
                    bootstrap_peer_config.peer_id,
                    format!("/ip4/{}", bootstrap_peer_config.ip)
                        .parse::<Multiaddr>()
                        .unwrap_or_else(|_| {
                            panic!("Wrong ip4 address format {}", bootstrap_peer_config.ip)
                        })
                        .with(multiaddr::Protocol::Tcp(bootstrap_peer_config.tcp_port)),
                ),
                identify: identify::Behaviour::new(identify::Config::new(
                    "/starknet/1".to_owned(),
                    public_key,
                )),
                kademlia: kad::Behaviour::with_config(
                    peer_id,
                    MemoryStore::new(peer_id),
                    kad_config,
                ),
                streamed_bytes: streamed_bytes::Behaviour::new(streamed_bytes_config),
            },
        }
    }
}
