mod behaviour;
pub mod executor;
/// This crate is responsible for sending messages to a given peer and responding to them according
/// to the [`Starknet p2p specs`]
///
/// [`Starknet p2p specs`]: https://github.com/starknet-io/starknet-p2p-specs/
mod get_blocks;
mod messages;
mod peer_manager;

use std::{
    collections::HashMap,
    task::{Context, Poll},
};

use crate::messages::block::GetBlocksResponse;
use crate::peer_manager::PeerManager;
use behaviour::SupportedBehaviours;
use executor::ExecutorsConfig;
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    StreamExt,
};
use libp2p::{
    core::transport::dummy::DummyTransport,
    swarm::{self, SwarmBuilder},
    PeerId, Swarm, Transport,
};

#[derive(PartialEq, Eq, Hash)]
enum ProtocolID {
    GetBlocks,
}

enum NetworkEvent {}

pub struct Network {
    peer_manager: PeerManager,
    swarm: Swarm<swarm::dummy::Behaviour>,
    requests_map: HashMap<(String, ProtocolID), UnboundedSender<GetBlocksResponse>>,
    executors: executor::Executors,
}

impl Network {
    // TODO(Nevo): define what query is
    fn get_blocks(&mut self, query: String) -> UnboundedReceiver<GetBlocksResponse> {
        let peer_requests = self.peer_manager.split_request_and_assign_peers(query);
        let (sender, receiver) = unbounded();
        for peer_request in peer_requests {
            // TODO(Nevo): implement behaviour and send request
            // let request_id = self.swarm.behaviour().send_request(peer_request);
            self.requests_map.insert((peer_request.peer_id, ProtocolID::GetBlocks), sender.clone());
        }
        receiver
    }

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<NetworkEvent> {
        match self.swarm.poll_next_unpin(cx) {
            Poll::Ready(Some(event)) => match event {
                _ => Poll::Pending,
            },
            Poll::Ready(None) => panic!("Swarm stream closed unexpectedly"),
            Poll::Pending => Poll::Pending,
        }
    }

    pub fn new(
        _behaviours_ordered: Vec<SupportedBehaviours>,
        executors_config: ExecutorsConfig,
    ) -> Self {
        // TODO(Nevo): build mixed behaviour based on the behaviours_ordered
        let mixed_behaviour = swarm::dummy::Behaviour;
        // TODO(nevo): build swarm properly, configure executor!
        let swarm = SwarmBuilder::without_executor(
            DummyTransport::new().boxed(),
            mixed_behaviour,
            PeerId::random(),
        )
        .build();
        Self {
            peer_manager: PeerManager {},
            swarm,
            requests_map: HashMap::new(),
            executors: executor::Executors::new(executors_config),
        }
    }
}
