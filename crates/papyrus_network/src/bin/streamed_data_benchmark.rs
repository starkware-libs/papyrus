use std::collections::HashMap;
use std::iter;
use std::str::FromStr;
use std::time::{Duration, Instant};

use clap::Parser;
use futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder};
use papyrus_network::messages::protobuf::stress_test_message::Msg;
use papyrus_network::messages::protobuf::{BasicMessage, InboundSessionStart, StressTestMessage};
use papyrus_network::streamed_data_protocol::behaviour::{Behaviour, Event, SessionError};
use papyrus_network::streamed_data_protocol::{
    Config,
    InboundSessionId,
    OutboundSessionId,
    SessionId,
};
use tokio::time::timeout;

fn pretty_size(mut size: f64) -> String {
    for term in ["B", "KB", "MB", "GB"] {
        if size < 1024.0 {
            return format!("{:.2} {}", size, term);
        }
        size = size / 1024.0;
    }
    return format!("{:.2} TB", size);
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

    /// Number of segments to divide each inbound session. Each segments results will be printed
    /// separately alongside a final summary.
    #[arg(short = 'g', long, default_value_t = 1)]
    num_segments: u64,

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
            swarm.behaviour_mut().send_query(BasicMessage { number }, peer_id).unwrap();
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
        .unwrap();
    let segment_size = args.num_messages_per_session / args.num_segments;
    for i in 0..args.num_messages_per_session {
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
            .unwrap();
        if i % segment_size == 0 {
            swarm
                .behaviour_mut()
                .send_data(
                    StressTestMessage {
                        msg: Some(Msg::SegmentFinished(BasicMessage { number: segment_size })),
                    },
                    inbound_session_id,
                )
                .unwrap();
        }
    }
    let last_segment_size = args.num_messages_per_session % segment_size;
    // If there is only one segment, there's no need to print its measurements since they'll appear
    // in the total measurements.
    if last_segment_size != 0 && args.num_segments > 1 {
        swarm
            .behaviour_mut()
            .send_data(
                StressTestMessage {
                    msg: Some(Msg::SegmentFinished(BasicMessage { number: last_segment_size })),
                },
                inbound_session_id,
            )
            .unwrap();
    }
    swarm.behaviour_mut().close_session(inbound_session_id.into()).unwrap();
}

struct OutboundSessionMeasurement {
    start_time: Instant,
    first_message_time: Option<Instant>,
    current_segment_start_time: Option<Instant>,
    segment_measurements: Vec<(Duration, u64)>,
    num_messages: Option<u64>,
    message_size: Option<u64>,
}

impl OutboundSessionMeasurement {
    pub fn print(&self) {
        let messages_elapsed = self.first_message_time.unwrap().elapsed();
        let elapsed = self.start_time.elapsed();
        let num_messages = self.num_messages.unwrap();
        let message_size = self.message_size.unwrap();
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
        println!("---- Message sending statistics across time ----");
        for (segment_elapsed, num_messages_in_segment) in &self.segment_measurements {
            println!(
                "{:.2} messages/second",
                *num_messages_in_segment as f64 / segment_elapsed.as_secs_f64()
            );
        }
    }

    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            first_message_time: None,
            current_segment_start_time: None,
            segment_measurements: vec![],
            num_messages: None,
            message_size: None,
        }
    }
    pub fn report_first_message(&mut self, inbound_session_start: InboundSessionStart) {
        self.first_message_time = Some(Instant::now());
        self.current_segment_start_time = self.first_message_time;
        self.num_messages = Some(inbound_session_start.num_messages);
        self.message_size = Some(inbound_session_start.message_size);
    }

    pub fn report_segment_finished(&mut self, basic_message: BasicMessage) {
        self.segment_measurements.push((
            self.current_segment_start_time
                .expect("Received SegmentFinished as the first message")
                .elapsed(),
            basic_message.number,
        ));
        self.current_segment_start_time = Some(Instant::now());
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let listen_address = Multiaddr::from_str(&args.listen_address)
        .expect(&format!("Unable to parse address {}", args.listen_address));

    let key_pair = Keypair::generate_ed25519();
    let mut swarm = SwarmBuilder::with_existing_identity(key_pair)
        .with_tokio()
        .with_quic()
        .with_behaviour(|_| {
            Behaviour::<BasicMessage, StressTestMessage>::new(Config {
                substream_timeout: Duration::from_secs(3600),
                protocol_name: StreamProtocol::new("/papyrus/bench/1"),
            })
        })
        .expect("Error while building the swarm")
        .with_swarm_config(|cfg| {
            cfg.with_idle_connection_timeout(Duration::from_secs(args.idle_connection_timeout))
        })
        .build();
    swarm
        .listen_on(listen_address)
        .expect(&format!("Error while binding to {}", args.listen_address));

    if let Some(dial_address_str) = args.dial_address.as_ref() {
        let dial_address = Multiaddr::from_str(dial_address_str)
            .expect(&format!("Unable to parse address {}", dial_address_str));
        swarm
            .dial(DialOpts::unknown_peer_id().address(dial_address).build())
            .expect(&format!("Error while dialing {}", dial_address_str));
    }

    let mut outbound_session_measurements = HashMap::new();
    let mut connected_in_the_past = false;
    loop {
        let maybe_event = timeout(Duration::from_secs(10), swarm.next()).await;
        let Ok(Some(event)) = maybe_event else {
            if !connected_in_the_past || swarm.network_info().num_peers() > 0 {
                continue;
            }
            break;
        };
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
            SwarmEvent::Behaviour(
                Event::SessionClosedByRequest {
                    session_id: SessionId::OutboundSessionId(outbound_session_id),
                }
                | Event::SessionClosedByPeer {
                    session_id: SessionId::OutboundSessionId(outbound_session_id),
                },
            ) => {
                outbound_session_measurements[&outbound_session_id].print();
            }
            SwarmEvent::Behaviour(Event::ReceivedData { outbound_session_id, data }) => {
                match data.msg {
                    Some(Msg::Start(inbound_session_start)) => {
                        outbound_session_measurements
                            .get_mut(&outbound_session_id)
                            .unwrap()
                            .report_first_message(inbound_session_start);
                    }
                    Some(Msg::SegmentFinished(basic_message)) => {
                        outbound_session_measurements
                            .get_mut(&outbound_session_id)
                            .unwrap()
                            .report_segment_finished(basic_message);
                    }
                    _ => {}
                }
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
            _ => {}
        }
    }
}
