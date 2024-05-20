use std::time::Duration;

use futures::{FutureExt, SinkExt, StreamExt};
use libp2p::core::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, Swarm};
use libp2p_swarm_test::SwarmExt;

use crate::gossipsub_impl::Topic;
use crate::mixed_behaviour::MixedBehaviour;
use crate::network_manager::GenericNetworkManager;
use crate::streamed_bytes;
use crate::streamed_bytes::Bytes;
use crate::test_utils::MockDBExecutor;

const TIMEOUT: Duration = Duration::from_secs(1);

async fn create_swarm(bootstrap_peer_multiaddr: Option<Multiaddr>) -> Swarm<MixedBehaviour> {
    let mut swarm = Swarm::new_ephemeral(|keypair| {
        MixedBehaviour::new(
            keypair.clone(),
            bootstrap_peer_multiaddr,
            streamed_bytes::Config::default(),
        )
    });
    // Not using SwarmExt::listen because it panics if the swarm emits other events
    let expected_listener_id = swarm.listen_on(Protocol::Memory(0).into()).unwrap();
    let address = swarm
        .wait(|event| match event {
            SwarmEvent::NewListenAddr { listener_id, address }
                if expected_listener_id == listener_id =>
            {
                Some(address)
            }
            _ => None,
        })
        .await;
    swarm.add_external_address(address);

    swarm
}

fn create_network_manager(
    swarm: Swarm<MixedBehaviour>,
) -> GenericNetworkManager<MockDBExecutor, Swarm<MixedBehaviour>> {
    GenericNetworkManager::generic_new(swarm, MockDBExecutor::default(), BUFFER_SIZE)
}

const BUFFER_SIZE: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Number(pub u8);

#[derive(Debug)]
struct EmptyBytesError;

impl TryFrom<Bytes> for Number {
    type Error = EmptyBytesError;

    fn try_from(bytes: Bytes) -> Result<Self, Self::Error> {
        bytes.first().map(|x| Number(*x)).ok_or(EmptyBytesError)
    }
}

impl From<Number> for Bytes {
    fn from(num: Number) -> Self {
        vec![num.0]
    }
}

#[tokio::test]
async fn broadcast_subscriber_end_to_end_test() {
    let topic1 = Topic::new("TOPIC1");
    let topic2 = Topic::new("TOPIC2");
    let bootstrap_swarm = create_swarm(None).await;
    let bootstrap_peer_multiaddr = bootstrap_swarm.external_addresses().next().unwrap().clone();
    let bootstrap_peer_multiaddr =
        bootstrap_peer_multiaddr.with_p2p(*bootstrap_swarm.local_peer_id()).unwrap();
    let bootstrap_network_manager = create_network_manager(bootstrap_swarm);
    let mut network_manager1 =
        create_network_manager(create_swarm(Some(bootstrap_peer_multiaddr.clone())).await);
    let mut network_manager2 =
        create_network_manager(create_swarm(Some(bootstrap_peer_multiaddr)).await);

    let mut subscriber_channels1_1 = network_manager1
        .register_broadcast_subscriber::<Number>(topic1.clone(), BUFFER_SIZE)
        .unwrap();
    let mut subscriber_channels1_2 = network_manager1
        .register_broadcast_subscriber::<Number>(topic2.clone(), BUFFER_SIZE)
        .unwrap();

    let subscriber_channels2_1 = network_manager2
        .register_broadcast_subscriber::<Number>(topic1.clone(), BUFFER_SIZE)
        .unwrap();

    let subscriber_channels2_2 = network_manager2
        .register_broadcast_subscriber::<Number>(topic2.clone(), BUFFER_SIZE)
        .unwrap();

    tokio::select! {
        _ = network_manager1.run() => panic!("network manager ended"),
        _ = network_manager2.run() => panic!("network manager ended"),
        _ = bootstrap_network_manager.run() => panic!("network manager ended"),
        result = tokio::time::timeout(
            TIMEOUT, async move {
                // TODO(shahak): Remove this sleep once we fix the bug of broadcasting while there
                // are no peers.
                tokio::time::sleep(Duration::from_millis(100)).await;
                let number1 = Number(1);
                let number2 = Number(2);
                let mut broadcasted_messages_receiver2_1 =
                    subscriber_channels2_1.broadcasted_messages_receiver;
                let mut broadcasted_messages_receiver2_2 =
                    subscriber_channels2_2.broadcasted_messages_receiver;
                subscriber_channels1_1.messages_to_broadcast_sender.send(number1).await.unwrap();
                subscriber_channels1_2.messages_to_broadcast_sender.send(number2).await.unwrap();
                let (received_number1, _report_callback) =
                    broadcasted_messages_receiver2_1.next().await.unwrap();
                let (received_number2, _report_callback) =
                    broadcasted_messages_receiver2_2.next().await.unwrap();
                assert_eq!(received_number1.unwrap(), number1);
                assert_eq!(received_number2.unwrap(), number2);
                assert!(broadcasted_messages_receiver2_1.next().now_or_never().is_none());
                assert!(broadcasted_messages_receiver2_2.next().now_or_never().is_none());
            }
        ) => {
            result.unwrap()
        }
    }
}
