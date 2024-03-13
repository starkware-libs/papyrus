use std::collections::HashMap;
use std::time::{Duration, Instant};

use clap::Parser;
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::{PeerId, StreamProtocol, Swarm};
use papyrus_network::bin_utils::{build_swarm, dial};
use papyrus_network::streamed_bytes::behaviour::{Behaviour, Event, SessionError};
use papyrus_network::streamed_bytes::{
    Bytes,
    Config,
    InboundSessionId,
    OutboundSessionId,
    SessionId,
};

const PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/papyrus/bench/1");
const CONST_BYTE: u8 = 1;

fn pretty_size(mut size: f64) -> String {
    for term in ["B", "KB", "MB", "GB"] {
        if size < 1024.0 {
            return format!("{:.2} {}", size, term);
        }
        size /= 1024.0;
    }
    format!("{:.2} TB", size)
}

fn encode_inbound_session_metadata(num_messages: usize, message_size: usize) -> Bytes {
    let mut result = num_messages.to_be_bytes().to_vec();
    result.extend_from_slice(&message_size.to_be_bytes());
    result
}

fn decode_inbound_session_metadata(mut bytes: Bytes) -> (usize, usize) {
    let second_bytes = bytes.split_off(8);
    (
        usize::from_be_bytes(
            bytes.try_into().expect("Failed converting a vec of size 8 to [u8; 8]"),
        ),
        usize::from_be_bytes(
            second_bytes
                .try_into()
                .expect("Called decode_inbound_session_metadata on Vec of size not 16"),
        ),
    )
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

    /// Amount of expected inbound sessions.
    #[arg(short = 'i', long)]
    num_expected_inbound_sessions: usize,

    /// Amount of expected peers to connect to this peer (dial or listener).
    #[arg(short = 'c', long)]
    num_expected_connections: usize,

    /// Number of queries to send for each node that we connect to (whether we dialed to it or it
    /// dialed to us).
    #[arg(short = 'q', long, default_value_t)]
    num_queries_per_connection: u64,

    /// Number of messages to send for each inbound session.
    #[arg(short = 'm', long, default_value_t)]
    num_messages_per_session: usize,

    /// Size (in bytes) of each message to send for inbound sessions.
    #[arg(short = 's', long, default_value_t)]
    message_size: usize,

    /// Amount of time (in seconds) to wait until closing an unactive connection.
    #[arg(short = 't', long, default_value_t = 10)]
    idle_connection_timeout: u64,
}

fn create_outbound_sessions_if_all_peers_connected(
    swarm: &mut Swarm<Behaviour>,
    peer_id: PeerId,
    outbound_session_measurements: &mut HashMap<OutboundSessionId, OutboundSessionMeasurement>,
    peers_pending_outbound_session: &mut Vec<PeerId>,
    args: &Args,
) {
    peers_pending_outbound_session.push(peer_id);
    if peers_pending_outbound_session.len() >= args.num_expected_connections {
        for peer_id in peers_pending_outbound_session {
            for _ in 0..args.num_queries_per_connection {
                let outbound_session_id =
                    swarm.behaviour_mut().send_query(vec![], *peer_id, PROTOCOL_NAME).expect(
                        "There's no connection to a peer immediately after we got a \
                         ConnectionEstablished event",
                    );
                outbound_session_measurements
                    .insert(outbound_session_id, OutboundSessionMeasurement::new());
            }
        }
    }
}

fn send_data_to_inbound_sessions(
    swarm: &mut Swarm<Behaviour>,
    inbound_session_to_messages: &mut HashMap<InboundSessionId, Vec<Vec<u8>>>,
    args: &Args,
) {
    for inbound_session_id in inbound_session_to_messages.keys() {
        swarm
            .behaviour_mut()
            .send_data(
                encode_inbound_session_metadata(args.num_messages_per_session, args.message_size),
                *inbound_session_id,
            )
            .unwrap_or_else(|_| {
                panic!("Inbound session {} dissappeared unexpectedly", inbound_session_id)
            });
    }
    while !inbound_session_to_messages.is_empty() {
        inbound_session_to_messages.retain(|inbound_session_id, messages| match messages.pop() {
            Some(message) => {
                swarm.behaviour_mut().send_data(message, *inbound_session_id).unwrap_or_else(
                    |_| panic!("Inbound session {} dissappeared unexpectedly", inbound_session_id),
                );

                true
            }
            None => {
                swarm.behaviour_mut().close_inbound_session(*inbound_session_id).unwrap_or_else(
                    |_| panic!("Inbound session {} dissappeared unexpectedly", inbound_session_id),
                );
                false
            }
        })
    }
}

// TODO(shahak) extract to other file.
struct OutboundSessionMeasurement {
    start_time: Instant,
    first_message_time: Option<Instant>,
    num_messages: Option<usize>,
    message_size: Option<usize>,
}

impl OutboundSessionMeasurement {
    pub fn print(&self) {
        let Some(first_message_time) = self.first_message_time else {
            println!(
                "An outbound session finished with no messages, skipping time measurements
display"
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
    pub fn report_first_message(&mut self, data: Bytes) {
        self.first_message_time = Some(Instant::now());
        let (num_messages, message_size) = decode_inbound_session_metadata(data);
        self.num_messages = Some(num_messages);
        self.message_size = Some(message_size);
    }
}

fn dial_if_requested(swarm: &mut Swarm<Behaviour>, args: &Args) {
    if let Some(dial_address) = args.dial_address.as_ref() {
        dial(swarm, dial_address);
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let config = Config {
        session_timeout: Duration::from_secs(3600),
        supported_inbound_protocols: vec![PROTOCOL_NAME],
    };
    let mut swarm = build_swarm(
        vec![args.listen_address.clone()],
        Duration::from_secs(args.idle_connection_timeout),
        Behaviour::new(config),
    );

    let mut outbound_session_measurements = HashMap::new();
    let mut inbound_session_to_messages = HashMap::new();
    let mut connected_in_the_past = false;

    let mut preprepared_messages = (0..args.num_expected_inbound_sessions)
        .map(|_| {
            (0..args.num_messages_per_session)
                .map(|_| vec![CONST_BYTE; args.message_size])
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let mut peers_pending_outbound_session = Vec::new();
    println!("Preprepared messages for sending");

    dial_if_requested(&mut swarm, &args);

    while let Some(event) = swarm.next().await {
        match event {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("Connected to a peer!");
                connected_in_the_past = true;
                create_outbound_sessions_if_all_peers_connected(
                    &mut swarm,
                    peer_id,
                    &mut outbound_session_measurements,
                    &mut peers_pending_outbound_session,
                    &args,
                );
            }
            SwarmEvent::Behaviour(Event::NewInboundSession { inbound_session_id, .. }) => {
                inbound_session_to_messages.insert(
                    inbound_session_id,
                    preprepared_messages
                        .pop()
                        .expect("There are more inbound sessions than expected"),
                );
                if preprepared_messages.is_empty() {
                    send_data_to_inbound_sessions(
                        &mut swarm,
                        &mut inbound_session_to_messages,
                        &args,
                    );
                }
            }
            SwarmEvent::Behaviour(Event::SessionFinishedSuccessfully {
                session_id: SessionId::OutboundSessionId(outbound_session_id),
            }) => {
                outbound_session_measurements[&outbound_session_id].print();
            }
            SwarmEvent::Behaviour(Event::ReceivedData { outbound_session_id, data }) => {
                if data[0] != CONST_BYTE {
                    outbound_session_measurements
                        .get_mut(&outbound_session_id)
                        .expect("Received data on non-existing outbound session")
                        .report_first_message(data);
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
            SwarmEvent::Behaviour(Event::SessionFinishedSuccessfully {
                session_id: SessionId::InboundSessionId(_),
            })
            | SwarmEvent::NewListenAddr { .. }
            | SwarmEvent::IncomingConnection { .. }
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
