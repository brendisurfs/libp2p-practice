use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::time::Duration;

use async_std::task;
use libp2p::futures::StreamExt;
use libp2p::gossipsub::{
    Gossipsub, GossipsubConfigBuilder, GossipsubEvent, GossipsubMessage, IdentTopic as Topic,
    MessageAuthenticity, MessageId,
};
use libp2p::identity::Keypair;
use libp2p::kad::store::MemoryStore;
use libp2p::kad::{Kademlia, KademliaConfig, KademliaEvent, QueryResult};
use libp2p::swarm::SwarmEvent;
use libp2p::{development_transport, Multiaddr, NetworkBehaviour, PeerId, Swarm};
use tracing::{debug, error, info, warn};

const BOOTNODE: &'static str = "QmeNkbyj4c33D4WuzwtNzdu65wsrEeHz7CZo9gv8nFtT2f";
const DIAL_ADDR: &'static str = "/ip4/192.168.1.67/tcp/59056";

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().init();

    info!("starting peer discovery ... ");

    let local_key = Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    let transport = development_transport(local_key.clone()).await?;

    let topic = Topic::new("tier_one");

    // swarm config
    let mut swarm = {
        // creating an id to content address messages.
        let message_id_fn = |message: &GossipsubMessage| {
            let mut hasher_state = DefaultHasher::new();
            message.data.hash(&mut hasher_state);
            MessageId::from(hasher_state.finish().to_string())
        };

        let gossip_config = GossipsubConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(10))
            .validation_mode(libp2p::gossipsub::ValidationMode::Strict)
            .message_id_fn(message_id_fn)
            .build()
            .expect("could not build a valid config");

        let mut gossipsub: Gossipsub =
            Gossipsub::new(MessageAuthenticity::Signed(local_key), gossip_config)
                .expect("correct config");

        gossipsub
            .subscribe(&topic)
            .expect("could not subscribe to topic");

        // let mut kad_config = KademliaConfig::default();
        // let store = MemoryStore::new(local_peer_id);
        // kad_config.set_query_timeout(Duration::from_secs(5 * 60));
        // let mut kad_behavior = Kademlia::with_config(local_peer_id, store, kad_config);

        // now we add our boot nodes.
        // kad_behavior.add_address(&PeerId::from_str(BOOTNODE)?, bootaddr.clone());

        Swarm::new(transport, gossipsub, local_peer_id)
    };

    // this is our endpoint, but we need to add that peer id.
    let bootaddr = Multiaddr::from_str(DIAL_ADDR)?;
    let bootnode_peer_id = PeerId::from_str(BOOTNODE)?;
    swarm.dial(bootaddr)?;

    let listener_id = swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    info!("this nodes listener id: {:?}", listener_id);

    task::block_on(async move {
        loop {
            let event = swarm.select_next_some().await;
            debug!("event: {:?}", event);
            match event {
                // handle on connection established
                SwarmEvent::ConnectionEstablished {
                    peer_id, endpoint, ..
                } => {
                    info!(
                        "new peer connection established: {:?} at endpoint: {:?}",
                        peer_id, endpoint
                    );

                    let peers = swarm.connected_peers().count();
                    info!("number of peers: {:?}", peers);
                }
                // handle on connection close
                SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                    warn!("connection closed: {:?} | cause: {:?}", peer_id, cause);
                }
                // match gossip behaviors
                SwarmEvent::Behaviour(gossip_event) => match gossip_event {
                    GossipsubEvent::Message {
                        propagation_source,
                        message_id,
                        message,
                    } => {
                        info!(
                            "prop source: {:?}, message id: {:?} , message: {:?}",
                            propagation_source, message_id, message
                        );
                    }
                    _ => (),
                },
                _ => (),
            } // end match
        }
    })
}
