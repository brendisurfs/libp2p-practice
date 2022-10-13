use std::error::Error;

use libp2p::core::transport::ListenerId;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::mdns::{Mdns, MdnsConfig, MdnsEvent};
use libp2p::swarm::{KeepAlive, SwarmEvent};
use libp2p::{identity, Multiaddr, NetworkBehaviour, PeerId, Swarm};
use tracing::{info, warn};

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    mdns: Mdns,
    keep_alive: KeepAlive,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().init();

    let mut private_as_bytes = tokio::fs::read("private.pk8").await?;
    let kp = identity::Keypair::rsa_from_pkcs8(&mut private_as_bytes)?;
    let peer_id = PeerId::from(kp.public());
    info!("peer id: {:?}", peer_id);

    // creating a transport
    let transport = libp2p::development_transport(kp).await?;
    let network_behavior = Mdns::new(MdnsConfig::default()).await?;

    // create a swarm that establishes connections through a given transport.
    // mdns behavior will not actually initiate any connections as its only UDP.
    let mut swarm = Swarm::new(transport, network_behavior, peer_id);
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
                info!("new connection established: {}", peer_id,);
            }
            // handle close connection.
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                info!("peer disconnected: {:?} | cause: {:?}", peer_id, cause);
            }
            // handle when a peer is discovered
            SwarmEvent::Behaviour(MdnsEvent::Discovered(peers)) => {
                let num_peers = peers.len();
                for (peer, addr) in peers {
                    info!("discovered {} {}", peer, addr);
                    info!("number of peers: {}", num_peers);
                }
            }
            SwarmEvent::Behaviour(MdnsEvent::Expired(expired)) => {
                let num_peers = expired.len();
                for (peer, addr) in expired {
                    warn!("expired {} {}", peer, addr);
                    info!("number of peers: {}", num_peers);
                }
            }
            _ => (),
        }
    }
}
