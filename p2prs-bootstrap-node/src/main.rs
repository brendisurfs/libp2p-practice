use async_std::fs;
use libp2p::futures::StreamExt;
use libp2p::gossipsub::{
    Gossipsub, GossipsubConfigBuilder, GossipsubEvent, GossipsubMessage, IdentTopic as Topic,
    MessageAuthenticity, MessageId, ValidationMode,
};
use libp2p::mdns::{Mdns, MdnsEvent, TokioMdns};
use libp2p::swarm::{SwarmBuilder, SwarmEvent};
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
#[derive(Debug)]
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

// // getcount from swarm, reduces retyping
// fn get_peer_count(swarm: &Swarm<CustomBehavior>) -> usize {
//     let ct = swarm.behaviour().all_peers().count();
//     ct
// }
const TOPICS: [&str; 3] = ["tier_one", "tier_two", "tier_three"];

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().init();
    info!("starting bootstrap node...");

    let mut private_as_bytes = async_std::fs::read("private.pk8").await?;
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

        let gossip_behavior: Gossipsub =
            Gossipsub::new(MessageAuthenticity::Signed(kp.clone()), gossip_config)
                .expect("could not build gossipsub");

        let mdns = Mdns::new(Default::default()).await?;
        let mut custom_behavior = CustomBehavior {
            gossipsub: gossip_behavior,
            mdns,
        };

        for topic in TOPICS {
            custom_behavior
                .gossipsub
                .subscribe(&Topic::new(topic))
                .expect("could not sub to topic");
        }
        // let subbed_topics: Vec<_> = gossip_behavior.topics().collect();
        // info!("subscribed to topics: {:?}", subbed_topics);
        // creating a transport
        let transport = libp2p::development_transport(kp).await?;

        SwarmBuilder::new(transport, custom_behavior, peer_id).build()
    };

    let addr: Multiaddr = "/ip4/192.168.1.67/tcp/59056".parse()?;
    info!("listening on: {:?}", &addr);
    // swarm.listen_on(addr)?;

    loop {
        let swarm_next_event = swarm.select_next_some().await;
        match swarm_next_event {
            SwarmEvent::ConnectionEstablished {
                peer_id,
                endpoint,
                num_established,
                concurrent_dial_errors,
            } => {
                info!("new peer connected: {:?}", peer_id);
            }

            SwarmEvent::NewListenAddr {
                listener_id,
                address,
            } => {
                info!("new listen addr: {:?}", listener_id);
            }

            SwarmEvent::Behaviour(bh) => match bh {
                BHEvent::Mdns(mdns_event) => match mdns_event {
                    MdnsEvent::Discovered(list) => {
                        let peers: Vec<_> = list.map(|p| p.0).collect();
                        peers
                            .iter()
                            .for_each(|p| swarm.behaviour_mut().gossipsub.add_explicit_peer(p));
                        info!("mdns event: {:#?}", peers);
                    }
                    _ => (),
                },
                BHEvent::Gossipsub(gossip_event) => {
                    info!("gossip event: {:?}", gossip_event);
                }
            },
            _ => info!("event not matched: {:?}", swarm_next_event),
        } // end swarm match
    } // end loop
}
