use libp2p::futures::StreamExt;
use libp2p::noise::{Keypair, NoiseConfig, X25519Spec};
use libp2p::swarm::NetworkBehaviour;
use libp2p::{
    core::upgrade,
    floodsub::{self, Floodsub, FloodsubEvent},
    identity,
    mdns::{MdnsEvent, TokioMdns},
    mplex,
    swarm::{SwarmBuilder, SwarmEvent},
    tcp::{GenTcpConfig, TokioTcpTransport},
    NetworkBehaviour, PeerId,
};
use libp2p::{Multiaddr, Transport};

use std::error::Error;
use std::str::FromStr;

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MyBehaviorEvent")]
struct MyBehavior {
    floodsub: Floodsub,
    mdns: TokioMdns,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum MyBehaviorEvent {
    Floodsub(FloodsubEvent),
    Mdns(MdnsEvent),
}

impl From<FloodsubEvent> for MyBehaviorEvent {
    fn from(event: FloodsubEvent) -> Self {
        MyBehaviorEvent::Floodsub(event)
    }
}

impl From<MdnsEvent> for MyBehaviorEvent {
    fn from(event: MdnsEvent) -> Self {
        MyBehaviorEvent::Mdns(event)
    }
}

#[derive(Debug)]
struct RNDRNode<'a> {
    id: &'a PeerId,
    stats: String,
}

const DIAL_ADDR: &'static str = "/ip4/192.168.1.67/tcp/0";
const BOOTNODE: &'static str = "QmeNkbyj4c33D4WuzwtNzdu65wsrEeHz7CZo9gv8nFtT2f";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // generate an identity keypair.
    let id_keys = identity::Keypair::generate_ed25519();
    let auth_keypair = Keypair::<X25519Spec>::new()
        .into_authentic(&id_keys)
        .expect("signing noise static keypair failed");

    let peer_id = PeerId::from(id_keys.public());
    println!("peer id: {:?}", &peer_id);

    // For testing purposes, create a bogus Node Id similar to how
    // I would set one up for RNDR p2p.
    let node_identifier = RNDRNode {
        id: &peer_id,
        stats: "RTX 3080Ti".to_string(),
    };
    // determine the topic to subscribe to.
    let tier_one_topic = floodsub::Topic::new("tier_one");

    let flood_topics = vec![
        floodsub::Topic::new("tier_one"),
        floodsub::Topic::new("tier_two"),
    ];

    let tcp_config = GenTcpConfig::default();
    let noise = NoiseConfig::xx(auth_keypair).into_authenticated();

    // use tokio transport to support async connection.s
    let transport = TokioTcpTransport::new(tcp_config)
        .upgrade(upgrade::Version::V1)
        .authenticate(noise)
        .multiplex(mplex::MplexConfig::new())
        .boxed();

    // SWARM CONFIGURATION
    // |
    // v
    let mut swarm = {
        let mdns = TokioMdns::new(Default::default()).await?;
        let mut behaviour = MyBehavior {
            floodsub: Floodsub::new(peer_id),
            mdns,
        };

        for topic in flood_topics.iter() {
            behaviour.floodsub.subscribe(topic.clone());
        }
        // /ip4/192.168.1.67/tcp/59056/QmeNkbyj4c33D4WuzwtNzdu65wsrEeHz7CZo9gv8nFtT2f
        // Now, we need to make a bot addr.
        // add a new address from a list | specified address we want.
        let bootstrap_peer_id = PeerId::from_str(BOOTNODE)?;
        let peer_retrieval = behaviour.mdns.addresses_of_peer(&bootstrap_peer_id);
        println!("retrieved peers: {:?}", peer_retrieval);

        SwarmBuilder::new(transport, behaviour, peer_id)
            .executor(Box::new(|fut| {
                tokio::spawn(fut);
            }))
            .build()
    }; // end swarm builder.

    // if another peer dials in.
    let bootaddr = Multiaddr::from_str(DIAL_ADDR)?;
    swarm.dial(bootaddr)?;

    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // start it all
    loop {
        match swarm.select_next_some().await {
            // listener has expired.
            SwarmEvent::ExpiredListenAddr {
                listener_id,
                address: _,
            } => {
                let expired_broadcast_msg = format!("listener id: {:?} has expired.", listener_id);
                println!("{}", expired_broadcast_msg);
                swarm
                    .behaviour_mut()
                    .floodsub
                    .publish(tier_one_topic.clone(), expired_broadcast_msg.as_bytes());
            }
            SwarmEvent::NewListenAddr {
                address,
                listener_id,
            } => {
                println!(
                    "new listener. listener id: {:?} | listening on: {:?}",
                    listener_id, address
                );
            }
            SwarmEvent::Behaviour(MyBehaviorEvent::Floodsub(FloodsubEvent::Message(message))) => {
                println!(
                    "message: {:?} from {:?}",
                    String::from_utf8_lossy(&message.data),
                    message.source
                );
            }

            // Custom events to match on.
            SwarmEvent::Behaviour(MyBehaviorEvent::Mdns(event)) => {
                match event {
                    // add a discovered node to the list of nodes to send out to.
                    MdnsEvent::Discovered(list) => {
                        let peers: Vec<_> = list
                            .map(|p| {
                                let (peer, _) = p;
                                swarm
                                    .behaviour_mut()
                                    .floodsub
                                    .add_node_to_partial_view(peer);
                                peer
                            })
                            .collect();
                        let node_ct = swarm.behaviour().mdns.discovered_nodes().len();
                        println!("node count: {:?}", node_ct);

                        swarm
                            .behaviour_mut()
                            .floodsub
                            .publish(tier_one_topic.clone(), "nice".as_bytes());
                    } // end peer discovery

                    MdnsEvent::Expired(list) => {
                        for (peer, _) in list {
                            println!("expired: {:?}", peer);
                            if !swarm.behaviour().mdns.has_node(&peer) {
                                swarm
                                    .behaviour_mut()
                                    .floodsub
                                    .remove_node_from_partial_view(&peer);
                            }
                        }
                    }
                }
            }
            _ => (),
        }
    }
}
