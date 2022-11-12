use async_std::stream::StreamExt;
use libp2p::identify::{Identify, IdentifyConfig};
use libp2p::rendezvous::{self, server::Event};
use libp2p::swarm::{DummyBehaviour, SwarmEvent};
use libp2p::NetworkBehaviour;
use libp2p::{development_transport, identify};
use libp2p::{identity, Swarm};
use libp2p::{ping, PeerId};
use void::Void;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();

    // defines 32 bytes of storage.
    let bytes = [0u8; 32];
    let key = identity::ed25519::SecretKey::from_bytes(bytes).expect("use 32 bytes ");
    let id = identity::Keypair::Ed25519(key.into());

    // build out a swarm.
    let mut swarm = {
        let transport = development_transport(id.clone()).await.unwrap();
        let behaviour = MyBehaviour {
            idenify: Identify::new(IdentifyConfig::new(
                "rendezvous-example/1.0.0".to_string(),
                id.public(),
            )),
            rendezvous: rendezvous::server::Behaviour::new(rendezvous::server::Config::default()),
            ping: ping::Behaviour::default(),
            keep_alive: DummyBehaviour::default(),
        };

        Swarm::new(transport, behaviour, PeerId::from(id.public()))
    };
    println!("local peer id: {:?}", swarm.local_peer_id());

    swarm
        .listen_on("/ip4/0.0.0.0/tcp/62649".parse().unwrap())
        .expect("could not listen on swarm");

    while let Some(event) = swarm.next().await {
        match event {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("connected to {}", peer_id);
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                println!("disconnected from {}", peer_id);
            }

            SwarmEvent::Behaviour(bhv) => match bhv {
                MyEvent::Rendezvous(rnd_event) => match rnd_event {
                    Event::DiscoverServed {
                        enquirer,
                        registrations,
                    } => {
                        println!(
                            "served peer {} with {} registrations",
                            enquirer,
                            registrations.len()
                        );
                    }
                    Event::PeerRegistered { peer, registration } => {
                        println!("peer registered: {:?} {:?}", peer, registration.namespace);
                    }
                    other_event => {
                        println!("unhandled: {:?}", other_event);
                    }
                },
                other => {
                    println!("unhandled: {:?}", other);
                }
            },
            other_event => println!("other event: {:?}", other_event),
        }
    }
    Ok(())
}

#[derive(NetworkBehaviour)]
#[behaviour(event_process = false)]
#[behaviour(out_event = "MyEvent")]
struct MyBehaviour {
    ping: ping::Behaviour,
    idenify: identify::Identify,
    keep_alive: libp2p::swarm::DummyBehaviour,
    rendezvous: rendezvous::server::Behaviour,
}

#[derive(Debug)]
enum MyEvent {
    Ping(ping::Event),
    Indentify(identify::IdentifyEvent),
    Rendezvous(rendezvous::server::Event),
}

impl From<rendezvous::server::Event> for MyEvent {
    fn from(event: rendezvous::server::Event) -> Self {
        MyEvent::Rendezvous(event)
    }
}

impl From<ping::Event> for MyEvent {
    fn from(event: ping::Event) -> Self {
        MyEvent::Ping(event)
    }
}

impl From<identify::IdentifyEvent> for MyEvent {
    fn from(event: identify::IdentifyEvent) -> Self {
        MyEvent::Indentify(event)
    }
}

impl From<Void> for MyEvent {
    fn from(event: Void) -> Self {
        void::unreachable(event)
    }
}
