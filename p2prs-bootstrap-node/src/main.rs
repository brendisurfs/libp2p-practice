use libp2p::futures::StreamExt;
use libp2p::gossipsub::{
    Gossipsub, GossipsubConfigBuilder, GossipsubEvent, GossipsubMessage, IdentTopic as Topic,
    MessageAuthenticity, MessageId, ValidationMode,
};
use libp2p::mdns::{Mdns, MdnsEvent};
use libp2p::swarm::SwarmEvent;
use libp2p::{identity, Multiaddr, NetworkBehaviour, PeerId, Swarm};
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use tracing::{info, warn};

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "BHEvent")]
struct CustomBehavior {
    gossipsub: Gossipsub,
    mdns: Mdns,
}
// this is where you reference out event.
enum BHEvent {
    Gossipsub(GossipsubEvent),
    Mdns(MdnsEvent),
}

impl From<GossipsubEvent> for BHEvent {
    fn from(event: GossipsubEvent) -> Self {
        BHEvent::Gossipsub(event)
    }
}

impl From<MdnsEvent> for BHEvent {
    fn from(event: MdnsEvent) -> Self {
        BHEvent::Mdns(event)
    }
}

// getcount from swarm, reduces retyping
fn get_peer_count(swarm: &Swarm<Gossipsub>) -> usize {
    let ct = swarm.behaviour().all_peers().count();
    ct
}

const TOPICS: [&str; 3] = ["tier_one", "tier_two", "tier_three"];

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().init();
    info!("starting bootstrap node...");

    let mut private_as_bytes = tokio::fs::read("private.pk8").await?;
    let kp = identity::Keypair::rsa_from_pkcs8(&mut private_as_bytes)?;
    let peer_id = PeerId::from(kp.public());
    info!("peer id: {:?}", peer_id);

    // create a swarm that establishes connections through a given transport.
    // mdns behavior will not actually initiate any connections as its only UDP.

    let mut swarm = {
        let message_id_fn = |message: &GossipsubMessage| -> _ {
            let mut state = DefaultHasher::new();
            message.data.hash(&mut state);
            MessageId::from(state.finish().to_string())
        };

        let gossip_config = GossipsubConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(10))
            .validation_mode(ValidationMode::Strict)
            .message_id_fn(message_id_fn)
            .build()
            .expect("could not build gossip config");

        let mut gossip_behavior: Gossipsub =
            Gossipsub::new(MessageAuthenticity::Signed(kp.clone()), gossip_config)
                .expect("could not build gossipsub");

        // creating a transport
        let transport = libp2p::development_transport(kp).await?;
        for topic in TOPICS {
            gossip_behavior
                .subscribe(&Topic::new(topic))
                .expect("could not sub to topic");
        }
        let subbed_topics: Vec<_> = gossip_behavior.topics().collect();
        info!("subscribed to topics: {:?}", subbed_topics);

        Swarm::new(transport, gossip_behavior, peer_id)
    };
    let addr: Multiaddr = "/ip4/192.168.1.67/tcp/59056".parse()?;
    info!("listening on: {:?}", &addr);
    swarm.listen_on(addr)?;

    loop {
        match swarm.select_next_some().await {
            // when a new listen adr is established.
            SwarmEvent::NewListenAddr {
                listener_id: _,
                address,
            } => {
                info!("new bootstrap id: {address}/{peer_id}");
            }
            SwarmEvent::ConnectionEstablished {
                peer_id,
                endpoint: _,
                num_established: _,
                concurrent_dial_errors: _,
            } => {
                info!("new connection established: {}", peer_id);
                let ct = get_peer_count(&swarm);
                info!("count: {:?}", ct);
            }
            // handle close connection.
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                warn!("peer disconnected: {:?} | cause: {:?}", peer_id, cause);
                let ct = get_peer_count(&swarm);
                info!("count: {:?}", ct);
            }
            SwarmEvent::Behaviour(gossip_event) => match gossip_event {
                GossipsubEvent::Message {
                    propagation_source: _,
                    message_id,
                    message,
                } => {
                    info!("message: id: {:?} | {:?}", message_id, message);
                }
                _ => (),
            },
            _ => (),
        }
    }
}

