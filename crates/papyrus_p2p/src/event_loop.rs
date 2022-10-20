use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

use futures::SinkExt;
use futures_channel::{mpsc, oneshot};
use libp2p::core::either::EitherError;
use libp2p::core::PeerId;
use libp2p::futures::StreamExt;
use libp2p::identify::IdentifyEvent;
use libp2p::multiaddr::Protocol;
use libp2p::ping::{Failure, PingEvent};
use libp2p::rendezvous::Cookie;
use libp2p::request_response::{RequestId, RequestResponseEvent, RequestResponseMessage};
use libp2p::swarm::{ConnectionHandlerUpgrErr, SwarmEvent};
use libp2p::{rendezvous, Multiaddr, Swarm};
use log::info;
use starknet_api::BlockHeader;

use crate::client::Request;
use crate::sync::{BlockHeaderRequest, BlockHeaderResponse};
use crate::{Event, MyBehaviour, MyEvent, NAMESPACE, RENDEZVOUS_POINT, RENDEZVOUS_POINT_ADDRESS};

pub struct EventLoop {
    swarm: Swarm<MyBehaviour>,
    command_receiver: mpsc::Receiver<Request>,
    event_sender: mpsc::Sender<Event>,
    pending_request_headers:
        HashMap<RequestId, oneshot::Sender<Result<BlockHeader, Box<dyn Error + Send>>>>,
    rendezvous_point_address: Multiaddr,
    rendezvous_point: PeerId,
    cookie: Option<Cookie>,
    peers: HashMap<PeerId, Multiaddr>,
}

impl EventLoop {
    pub fn new(
        swarm: Swarm<MyBehaviour>,
        command_receiver: mpsc::Receiver<Request>,
        event_sender: mpsc::Sender<Event>,
    ) -> Self {
        Self {
            swarm,
            command_receiver,
            event_sender,
            pending_request_headers: Default::default(),
            rendezvous_point_address: RENDEZVOUS_POINT_ADDRESS.parse::<Multiaddr>().unwrap(),
            rendezvous_point: RENDEZVOUS_POINT.parse().unwrap(),
            cookie: None,
            peers: Default::default(),
        }
    }

    pub async fn run(mut self) {
        let _ = self.swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap());
        self.swarm.dial(self.rendezvous_point_address.clone()).unwrap();
        let mut discover_tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tokio::select! {
                event = self.swarm.next() => self.handle_event(event.expect("Swarm stream to be infinite.")).await  ,
                request = self.command_receiver.next() => match request {
                    Some(request) => self.handle_request(request).await,
                    // Requests channel closed - shutting down the network event loop.
                    None=>  return,
                },
                _ = discover_tick.tick(), if self.cookie.is_some() => {
                    log::info!("Discovering nodes in {} namespace.", NAMESPACE);
                    self.swarm.behaviour_mut().rendezvous.discover(
                        Some(rendezvous::Namespace::new(NAMESPACE.to_string()).unwrap()),
                        self.cookie.clone(),
                        None,
                        self.rendezvous_point
                        )},
            }
        }
    }

    async fn handle_request(&mut self, request: Request) {
        match request {
            Request::RequestBlockHeader { peer, block_number, sender } => {
                info!("Request::RequestBlockHeader {peer}, {block_number}, {sender:?} ");
                let request_id = self
                    .swarm
                    .behaviour_mut()
                    .request_response
                    .send_request(&peer, BlockHeaderRequest { block_number });
                info!("request_id::request_id {request_id}");
                self.pending_request_headers.insert(request_id, sender);
            }
            Request::GetPeers { sender } => {
                info!("Request::GetPeers ");
                let _ = sender.send(self.peers.keys().cloned().collect());
            }
            Request::RespondBlockHeader { block_header, channel } => {
                info!("Request::RespondBlockHeader {block_header:?} {channel:?}");
                self.swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, BlockHeaderResponse(block_header))
                    .expect("Connection to peer to be still open.");
            }
        }
    }

    async fn handle_event(
        &mut self,
        event: SwarmEvent<
            MyEvent,
            EitherError<
                EitherError<EitherError<std::io::Error, Failure>, void::Void>,
                ConnectionHandlerUpgrErr<std::io::Error>,
            >,
        >,
    ) {
        match event {
            SwarmEvent::Behaviour(MyEvent::Ping(PingEvent { peer, result })) => {
                log::debug!("Ping event: peer - {}, result - {:#?}.", peer, result);
            }
            SwarmEvent::Behaviour(MyEvent::Identify(identify_eventeve)) => {
                match *identify_eventeve {
                    IdentifyEvent::Received { peer_id, info } => {
                        log::debug!(
                            "IdentifyEvent::Received info {:#?} from peer_id {}.",
                            info,
                            peer_id
                        );
                        if peer_id == self.rendezvous_point {
                            log::info!("Registering to namespace {}.", NAMESPACE);
                            self.swarm.behaviour_mut().rendezvous.register(
                                rendezvous::Namespace::from_static(NAMESPACE),
                                self.rendezvous_point,
                                Some(7200),
                            );
                        }
                    }
                    IdentifyEvent::Sent { peer_id } => {
                        log::debug!("IdentifyEvent::Sent from peer_id {}", peer_id);
                    }
                    IdentifyEvent::Pushed { peer_id: _ } => todo!(),
                    IdentifyEvent::Error { peer_id: _, error: _ } => todo!(),
                };
            }
            SwarmEvent::Behaviour(MyEvent::Rendezvous(rendezvous::client::Event::Registered {
                namespace,
                ttl,
                rendezvous_node,
            })) => {
                log::info!(
                    "Registered for namespace '{}' at rendezvous point {} for the next {} seconds",
                    namespace,
                    rendezvous_node,
                    ttl
                );
            }
            SwarmEvent::Behaviour(MyEvent::Rendezvous(rendezvous::client::Event::Discovered {
                registrations,
                cookie: new_cookie,
                ..
            })) => {
                log::info!("Discovered {} peers.", registrations.len());
                self.cookie.replace(new_cookie);

                for registration in registrations {
                    for address in registration.record.addresses() {
                        let peer = registration.record.peer_id();
                        log::info!("Discovered peer {} at {}", peer, address);

                        let p2p_suffix = Protocol::P2p(*peer.as_ref());
                        let address_with_p2p =
                            if !address.ends_with(&Multiaddr::empty().with(p2p_suffix.clone())) {
                                address.clone().with(p2p_suffix)
                            } else {
                                address.clone()
                            };
                        self.peers.insert(peer, address_with_p2p);
                        let peers = &self.peers;
                        info!("{peers:?}");
                        // self.swarm.dial(address_with_p2p).unwrap()
                    }
                }
            }
            SwarmEvent::Behaviour(MyEvent::RequestResponse(RequestResponseEvent::Message {
                message,
                // peer,
                ..
            })) => match message {
                RequestResponseMessage::Request { request, channel, .. } => {
                    info!("got a request event {request:?}");
                    self.event_sender
                        .send(Event::InboundRequest { request: request.block_number, channel })
                        .await
                        .expect("Event receiver not to be dropped.");
                }
                RequestResponseMessage::Response { request_id, response } => {
                    let _ = self
                        .pending_request_headers
                        .remove(&request_id)
                        .expect("Request to still be pending.")
                        .send(Ok(response.0));
                }
            },
            SwarmEvent::Behaviour(MyEvent::RequestResponse(
                RequestResponseEvent::ResponseSent { peer, request_id },
            )) => {
                log::info!("Response for request {request_id} sent to {peer}.");
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                let local_peer_id = *self.swarm.local_peer_id();
                log::info!(
                    "Local node is listening on {:?}",
                    address.with(Protocol::P2p(local_peer_id.into()))
                );
            }
            SwarmEvent::IncomingConnection { .. } => {}
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                log::info!("Connection established {peer_id:?}, {endpoint:?}.");
                if peer_id == self.rendezvous_point {
                    log::info!("Discovering nodes in {} namespace.", NAMESPACE);
                    self.swarm.behaviour_mut().rendezvous.discover(
                        Some(rendezvous::Namespace::new(NAMESPACE.to_string()).unwrap()),
                        None,
                        None,
                        self.rendezvous_point,
                    );
                }
            }
            SwarmEvent::ConnectionClosed { .. } => {}
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                log::error!("Outgoing connection error {peer_id:?}, {error:?}.");
                panic!("Outgoing connection error {peer_id:?}, {error:?}.");
            }
            SwarmEvent::IncomingConnectionError { .. } => {}
            SwarmEvent::Dialing(peer_id) => eprintln!("Dialing {}", peer_id),
            e => panic!("{:?}", e),
        }
    }
}
