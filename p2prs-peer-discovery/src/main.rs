use std::error::Error;
use std::str::FromStr;
use std::time::Duration;

use async_std::task;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::kad::store::MemoryStore;
use libp2p::kad::{Kademlia, KademliaConfig, KademliaEvent, QueryResult};
use libp2p::swarm::SwarmEvent;
use libp2p::{development_transport, Multiaddr, PeerId, Swarm};
use tracing::{error, info};

const BOOTNODE: &'static str = "QmeNkbyj4c33D4WuzwtNzdu65wsrEeHz7CZo9gv8nFtT2f";

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().init();

    info!("starting peer discovery ... ");

    let local_key = Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    let transport = development_transport(local_key).await?;

    // swarm config
    let mut swarm = {
        let mut kad_config = KademliaConfig::default();
        let store = MemoryStore::new(local_peer_id);
        kad_config.set_query_timeout(Duration::from_secs(5 * 60));
        let mut kad_behavior = Kademlia::with_config(local_peer_id, store, kad_config);
        // now we add our boot nodes.
        let bootaddr = Multiaddr::from_str("/ip4/192.168.1.67/tcp/59056")?;
        kad_behavior.add_address(&PeerId::from_str(BOOTNODE)?, bootaddr.clone());

        Swarm::new(transport, kad_behavior, local_peer_id)
    };

    // search for a peer
    let to_search: PeerId = Keypair::generate_ed25519().public().into();
    info!("searching for closest peers to {:?}", to_search);
    swarm.behaviour_mut().get_closest_peers(to_search);

    task::block_on(async move {
        loop {
            let event = swarm.select_next_some().await;
            match event {
                SwarmEvent::Behaviour(KademliaEvent::OutboundQueryCompleted {
                    result: QueryResult::GetClosestPeers(result),
                    ..
                }) => match result {
                    Ok(good_result) => {
                        if !good_result.peers.is_empty() {
                            println!("closest peers: {:?}", good_result.peers);
                        } else {
                            println!("no peers available");
                        }
                    }
                    Err(why) => {
                        error!("why: {:?}", why);
                    }
                },
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    info!("new connection! peer id: {peer_id:?}");
                }
                _ => (),
            } // end match
        }
    })
}
