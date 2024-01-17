use std::collections::HashMap;
use std::iter;
use std::str::FromStr;
use std::time::{Duration, Instant};

use clap::Parser;
use futures::StreamExt;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, StreamProtocol, Swarm};
use papyrus_network::bin_utils::build_swarm;
use papyrus_network::messages::protobuf::stress_test_message::Msg;
use papyrus_network::messages::protobuf::{BasicMessage, InboundSessionStart, StressTestMessage};
use papyrus_network::streamed_data::behaviour::{Behaviour, Event, SessionError};
use papyrus_network::streamed_data::{Config, InboundSessionId, SessionId};

fn pretty_size(mut size: f64) -> String {
    for term in ["B", "KB", "MB", "GB"] {
        if size < 1024.0 {
            return format!("{:.2} {}", size, term);
        }
        size /= 1024.0;
    }
    format!("{:.2} TB", size)
}

// TODO(shahak) extract to other file.
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

fn print_outbound_session_metrics(elapsed: Duration, num_messages: u64, message_size: u64) {
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

fn dial_if_requested(swarm: &mut Swarm<Behaviour<BasicMessage, StressTestMessage>>, args: &Args) {
    if let Some(dial_address_str) = args.dial_address.as_ref() {
        let dial_address = Multiaddr::from_str(dial_address_str)
            .unwrap_or_else(|_| panic!("Unable to parse address {}", dial_address_str));
        swarm
            .dial(DialOpts::unknown_peer_id().address(dial_address).build())
            .unwrap_or_else(|_| panic!("Error while dialing {}", dial_address_str));
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
    while let Some(event) = swarm.next().await {
        match event {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("Connected to a peer!");
                for number in 0..args.num_queries_per_connection {
                    swarm.behaviour_mut().send_query(BasicMessage { number }, peer_id).expect(
                        "There's no connection to a peer immediately after we got a \
                         ConnectionEstablished event",
                    );
                }
            }
            SwarmEvent::Behaviour(Event::NewInboundSession { inbound_session_id, .. }) => {
                send_data_to_inbound_session(&mut swarm, inbound_session_id, &args);
            }
            SwarmEvent::Behaviour(Event::SessionClosedByPeer {
                session_id: SessionId::OutboundSessionId(outbound_session_id),
            }) => {
                let OutboundSessionMeasurement { start_time, num_messages, message_size } =
                    outbound_session_measurements[&outbound_session_id];
                print_outbound_session_metrics(start_time.elapsed(), num_messages, message_size);
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
            SwarmEvent::OutgoingConnectionError { .. } => {
                dial_if_requested(&mut swarm, &args);
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
    }
}
