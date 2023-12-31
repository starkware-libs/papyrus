use std::collections::HashMap;
use std::iter;
use std::str::FromStr;
use std::time::{Duration, Instant};

use clap::Parser;
use futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, StreamProtocol, SwarmBuilder};
use papyrus_network::messages::protobuf::stress_test_message::Msg;
use papyrus_network::messages::protobuf::{BasicMessage, InboundSessionStart, StressTestMessage};
use papyrus_network::streamed_data_protocol::behaviour::{Behaviour, Event, SessionError};
use papyrus_network::streamed_data_protocol::{Config, SessionId};

fn pretty_size(mut size: f64) -> String {
    for term in ["B", "KB", "MB", "GB"] {
        if size < 1024.0 {
            return format!("{:.2} {}", size, term);
        }
        size = size / 1024.0;
    }
    return format!("{:.2} TB", size);
}

struct OutboundSessionMeasurement {
    pub start_time: Instant,
    pub num_messages: u64,
    pub message_size: u64,
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
    while let Some(event) = swarm.next().await {
        match event {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("Connected to a peer!");
                for number in 0..args.num_queries_per_connection {
                    swarm.behaviour_mut().send_query(BasicMessage { number }, peer_id).unwrap();
                }
            }
            SwarmEvent::Behaviour(Event::NewInboundSession { inbound_session_id, .. }) => {
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
                        .unwrap();
                }
                swarm.behaviour_mut().close_session(inbound_session_id.into()).unwrap();
            }
            SwarmEvent::Behaviour(
                Event::SessionClosedByRequest {
                    session_id: SessionId::OutboundSessionId(outbound_session_id),
                }
                | Event::SessionClosedByPeer {
                    session_id: SessionId::OutboundSessionId(outbound_session_id),
                },
            ) => {
                let OutboundSessionMeasurement { start_time, num_messages, message_size } =
                    outbound_session_measurements[&outbound_session_id];
                let elapsed = start_time.elapsed();
                println!("---------- Outbound session finished ----------");
                println!(
                    "Session had {} messages of size {}. In total {}",
                    num_messages,
                    pretty_size(message_size as f64),
                    pretty_size((message_size * num_messages) as f64),
                );
                println!("Session took {:.3} seconds", elapsed.as_secs_f64());
                println!("{:.2} messages/second", num_messages as f64 / elapsed.as_secs_f64());
                println!(
                    "{}/second",
                    pretty_size((message_size * num_messages) as f64 / elapsed.as_secs_f64())
                );
            }
            SwarmEvent::Behaviour(Event::ReceivedData { outbound_session_id, data }) => {
                if let Some(Msg::Start(InboundSessionStart { num_messages, message_size })) =
                    data.msg
                {
                    outbound_session_measurements.insert(
                        outbound_session_id,
                        OutboundSessionMeasurement {
                            start_time: Instant::now(),
                            num_messages,
                            message_size,
                        },
                    );
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
