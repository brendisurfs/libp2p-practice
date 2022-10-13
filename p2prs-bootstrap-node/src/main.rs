use std::error::Error;

use libp2p::futures::StreamExt;
use libp2p::mdns::{Mdns, MdnsConfig, MdnsEvent};
use libp2p::swarm::SwarmEvent;
use libp2p::{identity, PeerId, Swarm};
use tracing::{info, warn};
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().init();

    let id_keys = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(id_keys.public());

    info!("peer id: {:?}", peer_id);

    // creating a transport
    let transport = libp2p::development_transport(id_keys).await?;
    let network_behavior = Mdns::new(MdnsConfig::default()).await?;

    // create a swarm that establishes connections through a given transport.
    // mdns behavior will not actually initiate any connections as its only UDP.
    let mut swarm = Swarm::new(transport, network_behavior, peer_id);
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::ConnectionEstablished {
                peer_id,
                endpoint: _,
                num_established,
                concurrent_dial_errors: _,
            } => {
                info!(
                    "new connection established: {} {} ",
                    peer_id, num_established
                );
            }
            // handle when a peer is discovered
            SwarmEvent::Behaviour(MdnsEvent::Discovered(peers)) => {
                for (peer, addr) in peers {
                    info!("discovered {} {}", peer, addr);
                }
            }
            SwarmEvent::Behaviour(MdnsEvent::Expired(expired)) => {
                for (peer, addr) in expired {
                    warn!("expired {} {}", peer, addr);
                }
            }
            _ => (),
        }
    }
}
