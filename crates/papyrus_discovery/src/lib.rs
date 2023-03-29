#[cfg(test)]
mod discovery_test;
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::thread::sleep;
use std::time::{Duration, Instant};

use async_std::io;
use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::prelude::{AsyncRead, AsyncWrite};
use futures::stream::Next;
use futures::StreamExt;
use libp2p::core::upgrade::{
    read_varint, write_varint, InboundUpgrade, OutboundUpgrade, ProtocolName, UpgradeInfo,
};
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::handler::ConnectionEvent;
use libp2p::swarm::{
    ConnectionHandler, ConnectionHandlerEvent, ConnectionId, FromSwarm, KeepAlive,
    NetworkBehaviour, NetworkBehaviourAction, NotifyHandler, PollParameters, SubstreamProtocol,
    Swarm, SwarmEvent,
};
use libp2p::{request_response, Multiaddr, PeerId};
use tokio::join;

#[derive(Debug)]
pub struct DiscoveryRequest(u64);
#[derive(Debug)]
pub struct DiscoveryResponse(u64);

#[derive(Clone, Debug)]
pub struct ProtocolId {}

pub struct DiscoveryOutboundProtocol {
    pub request: u64,
}

impl ProtocolName for ProtocolId {
    fn protocol_name(&self) -> &[u8] {
        "/discovery/1".as_bytes()
    }
}

#[derive(Clone)]
pub struct DiscoveryCodec {}

#[async_trait]
impl request_response::Codec for DiscoveryCodec {
    type Protocol = ProtocolId;
    type Request = DiscoveryRequest;
    type Response = DiscoveryResponse;

    async fn read_request<T>(&mut self, _: &ProtocolId, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let num = read_varint(io).await? as u64;

        Ok(DiscoveryRequest(num))
    }

    async fn read_response<T>(&mut self, _: &ProtocolId, io: &mut T) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let num = read_varint(io).await? as u64;

        Ok(DiscoveryResponse(num))
    }

    async fn write_request<T>(
        &mut self,
        _: &ProtocolId,
        io: &mut T,
        DiscoveryRequest(num): DiscoveryRequest,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_varint(io, num as usize).await?;
        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &ProtocolId,
        io: &mut T,
        DiscoveryResponse(num): DiscoveryResponse,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_varint(io, num as usize).await?;
        Ok(())
    }
}

pub struct DiscoveryClient {
    other_peer_id: PeerId,
    other_peer_address: Multiaddr,
}

impl DiscoveryClient {
    pub async fn run(&self, mut swarm: Swarm<request_response::Behaviour<DiscoveryCodec>>) {
        let mut gathered_responses = HashSet::new();
        print!("Starting Client contacting peer {}\n", self.other_peer_id);
        swarm.behaviour_mut().add_address(&self.other_peer_id, self.other_peer_address.clone());
        for i in 0..10 {
            print!("Sending request no. {} to {}\n", i, self.other_peer_id);
            swarm.behaviour_mut().send_request(&self.other_peer_id, DiscoveryRequest(i));
        }
        loop {
            futures::select! {
                swarm_event = swarm.next() => {
                    match swarm_event {
                        Some(SwarmEvent::Behaviour(request_response::Event::Message{peer, message})) => {
                            match message {
                                request_response::Message::Request{request_id, request, channel} => {
                                    print!("Got request {} from peer {}\n", request.0, peer);
                                    swarm
                                        .behaviour_mut()
                                        .send_response(channel, DiscoveryResponse(request.0));
                                }
                                request_response::Message::Response{request_id, response} => {
                                    print!("Got response {} from peer {}\n", response.0, peer);
                                    gathered_responses.insert(response.0);
                                    if Self::is_done(&mut gathered_responses) {
                                        break;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn is_done(gathered_responses: &mut HashSet<u64>) -> bool {
        for i in 0..10 {
            if !gathered_responses.contains(&i) {
                return false;
            }
        }
        true
    }
}
