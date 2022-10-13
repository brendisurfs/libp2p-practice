use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::time::Duration;

use async_std::task;
use libp2p::floodsub::FloodsubEvent;
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
use tracing::{error, info};

const BOOTNODE: &'static str = "QmeNkbyj4c33D4WuzwtNzdu65wsrEeHz7CZo9gv8nFtT2f";

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().init();

    info!("starting peer discovery ... ");

    let local_key = Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    let transport = development_transport(local_key.clone()).await?;

    let topic = Topic::new("test-net");

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

        gossipsub.add_explicit_peer(&PeerId::from_str(BOOTNODE)?);

        // let mut kad_config = KademliaConfig::default();
        // let store = MemoryStore::new(local_peer_id);
        // kad_config.set_query_timeout(Duration::from_secs(5 * 60));
        // let mut kad_behavior = Kademlia::with_config(local_peer_id, store, kad_config);

        // now we add our boot nodes.
        let bootaddr = Multiaddr::from_str("/ip4/192.168.1.67/tcp/59056")?;
        // kad_behavior.add_address(&PeerId::from_str(BOOTNODE)?, bootaddr.clone());

        Swarm::new(transport, gossipsub, local_peer_id)
    };

    // search for a peer
    // let to_search: PeerId = Keypair::generate_ed25519().public().into();
    // info!("searching for closest peers to {:?}", to_search);
    // swarm.behaviour_mut().get_closest_peers(to_search);

    task::block_on(async move {
        loop {
            let event = swarm.select_next_some().await;
            match event {
                // handle on connection established
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    info!("new peer connection established: {:?}", peer_id);
                }
                // handle on connection close
                _ => (),
            } // end match
        }
    })
}
