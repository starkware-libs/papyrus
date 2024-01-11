use std::collections::HashMap;
use std::iter;
use std::time::{Duration, Instant};

use clap::Parser;
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::{PeerId, StreamProtocol, Swarm};
use papyrus_network::bin_utils::{build_swarm, dial};
use papyrus_network::messages::protobuf::stress_test_message::Msg;
use papyrus_network::messages::protobuf::{BasicMessage, InboundSessionStart, StressTestMessage};
use papyrus_network::streamed_data::behaviour::{Behaviour, Event, SessionError};
use papyrus_network::streamed_data::{Config, InboundSessionId, OutboundSessionId, SessionId};

fn pretty_size(mut size: f64) -> String {
    for term in ["B", "KB", "MB", "GB"] {
        if size < 1024.0 {
            return format!("{:.2} {}", size, term);
        }
        size /= 1024.0;
    }
    format!("{:.2} TB", size)
}

/// A node that benchmarks the throughput of messages sent/received.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Address this node listens on for incoming connections.
    #[arg(short, long)]
    listen_address: String,

    /// Address this node attempts to dial to.
    #[arg(short, long)]
    dial_address: Option<String>,

    /// Number of queries to send for each node that we connect to (whether we dialed to it or it
    /// dialed to us).
    #[arg(short = 'q', long, default_value_t)]
    num_queries_per_connection: u64,

    /// Number of messages to send for each inbound session.
    #[arg(short = 'm', long, default_value_t)]
    num_messages_per_session: u64,

    /// Size (in bytes) of each message to send for inbound sessions.
    #[arg(short = 's', long, default_value_t)]
    message_size: u64,

    /// Amount of time (in seconds) to wait until closing an unactive connection.
    #[arg(short = 't', long, default_value_t = 1)]
    idle_connection_timeout: u64,
}

fn create_outbound_sessions(
    swarm: &mut Swarm<Behaviour<BasicMessage, StressTestMessage>>,
    peer_id: PeerId,
    outbound_session_measurements: &mut HashMap<OutboundSessionId, OutboundSessionMeasurement>,
    args: &Args,
) {
    for number in 0..args.num_queries_per_connection {
        let outbound_session_id =
            swarm.behaviour_mut().send_query(BasicMessage { number }, peer_id).expect(
                "There's no connection to a peer immediately after we got a ConnectionEstablished \
                 event",
            );
        outbound_session_measurements
            .insert(outbound_session_id, OutboundSessionMeasurement::new());
    }
}

fn send_data_to_inbound_session(
    swarm: &mut Swarm<Behaviour<BasicMessage, StressTestMessage>>,
    inbound_session_id: InboundSessionId,
    args: &Args,
) {
    swarm
        .behaviour_mut()
        .send_data(
            StressTestMessage {
                msg: Some(Msg::Start(InboundSessionStart {
                    num_messages: args.num_messages_per_session,
                    message_size: args.message_size,
                })),
            },
            inbound_session_id,
        )
        .unwrap_or_else(|_| {
            panic!("Inbound session {} dissappeared unexpectedly", inbound_session_id)
        });
    for _ in 0..args.num_messages_per_session {
        swarm
            .behaviour_mut()
            .send_data(
                StressTestMessage {
                    msg: Some(Msg::Content(
                        iter::repeat(1u8).take(args.message_size as usize).collect(),
                    )),
                },
                inbound_session_id,
            )
            .unwrap_or_else(|_| {
                panic!("Inbound session {} dissappeared unexpectedly", inbound_session_id)
            });
    }
    swarm.behaviour_mut().close_session(inbound_session_id.into()).unwrap_or_else(|_| {
        panic!("Inbound session {} dissappeared unexpectedly", inbound_session_id)
    });
}

// TODO(shahak) extract to other file.
struct OutboundSessionMeasurement {
    start_time: Instant,
    first_message_time: Option<Instant>,
    num_messages: Option<u64>,
    message_size: Option<u64>,
}

impl OutboundSessionMeasurement {
    pub fn print(&self) {
        let Some(first_message_time) = self.first_message_time else {
            println!(
                "An outbound session finished with no messages, skipping time measurements display"
            );
            return;
        };
        let messages_elapsed = first_message_time.elapsed();
        let elapsed = self.start_time.elapsed();
        let num_messages = self.num_messages.expect(
            "OutboundSessionMeasurement's first_message_time field was set while the num_messages \
             field wasn't set",
        );
        let message_size = self.message_size.expect(
            "OutboundSessionMeasurement's first_message_time field was set while the message_size \
             field wasn't set",
        );
        println!("########## Outbound session finished ##########");
        println!(
            "Session had {} messages of size {}. In total {}",
            num_messages,
            pretty_size(message_size as f64),
            pretty_size((message_size * num_messages) as f64),
        );
        println!("Session took {:.3} seconds", elapsed.as_secs_f64());
        println!("Message sending took {:.3} seconds", messages_elapsed.as_secs_f64());
        println!("---- Total session statistics ----");
        println!("{:.2} messages/second", num_messages as f64 / elapsed.as_secs_f64());
        println!(
            "{}/second",
            pretty_size((message_size * num_messages) as f64 / elapsed.as_secs_f64())
        );
        println!("---- Message sending statistics ----");
        println!("{:.2} messages/second", num_messages as f64 / messages_elapsed.as_secs_f64());
        println!(
            "{}/second",
            pretty_size((message_size * num_messages) as f64 / messages_elapsed.as_secs_f64())
        );
    }

    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            first_message_time: None,
            num_messages: None,
            message_size: None,
        }
    }
    pub fn report_first_message(&mut self, inbound_session_start: InboundSessionStart) {
        self.first_message_time = Some(Instant::now());
        self.num_messages = Some(inbound_session_start.num_messages);
        self.message_size = Some(inbound_session_start.message_size);
    }
}

fn dial_if_requested(swarm: &mut Swarm<Behaviour<BasicMessage, StressTestMessage>>, args: &Args) {
    if let Some(dial_address) = args.dial_address.as_ref() {
        dial(swarm, dial_address);
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let config = Config {
        substream_timeout: Duration::from_secs(3600),
        protocol_name: StreamProtocol::new("/papyrus/bench/1"),
    };
    let mut swarm = build_swarm(args.listen_address.clone(), args.idle_connection_timeout, config);
    dial_if_requested(&mut swarm, &args);

    let mut outbound_session_measurements = HashMap::new();
    let mut connected_in_the_past = false;
    while let Some(event) = swarm.next().await {
        match event {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("Connected to a peer!");
                connected_in_the_past = true;
                create_outbound_sessions(
                    &mut swarm,
                    peer_id,
                    &mut outbound_session_measurements,
                    &args,
                );
            }
            SwarmEvent::Behaviour(Event::NewInboundSession { inbound_session_id, .. }) => {
                send_data_to_inbound_session(&mut swarm, inbound_session_id, &args);
            }
            SwarmEvent::Behaviour(Event::SessionClosedByPeer {
                session_id: SessionId::OutboundSessionId(outbound_session_id),
            }) => {
                outbound_session_measurements[&outbound_session_id].print();
            }
            SwarmEvent::Behaviour(Event::ReceivedData { outbound_session_id, data }) => {
                if let Some(Msg::Start(inbound_session_start)) = data.msg {
                    outbound_session_measurements
                        .get_mut(&outbound_session_id)
                        .expect("Received data on non-existing outbound session")
                        .report_first_message(inbound_session_start);
                }
            }
            SwarmEvent::OutgoingConnectionError { .. } => {
                dial_if_requested(&mut swarm, &args);
            }
            SwarmEvent::Behaviour(Event::SessionFailed {
                session_id,
                error: SessionError::ConnectionClosed,
            }) => {
                println!(
                    "Session {:?} failed on ConnectionClosed. Try to increase \
                     idle_connection_timeout",
                    session_id
                );
            }
            SwarmEvent::Behaviour(Event::SessionFailed {
                session_id,
                error: SessionError::IOError(io_error),
            }) => {
                println!("Session {:?} failed on {}", session_id, io_error.kind());
            }
            SwarmEvent::Behaviour(Event::SessionClosedByPeer {
                session_id: SessionId::InboundSessionId(inbound_session_id),
            }) => {
                println!("Outbound peer closed before us in session {:?}", inbound_session_id);
            }
            SwarmEvent::NewListenAddr { .. }
            | SwarmEvent::IncomingConnection { .. }
            | SwarmEvent::Behaviour(Event::SessionClosedByRequest {
                session_id: SessionId::InboundSessionId(..),
            })
            | SwarmEvent::ConnectionClosed { .. } => {}
            _ => {
                panic!("Unexpected event {:?}", event);
            }
        }
        if connected_in_the_past && swarm.network_info().num_peers() == 0 {
            break;
        }
    }
}
