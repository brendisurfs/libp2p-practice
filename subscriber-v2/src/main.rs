use std::time::Duration;

use futures_util::StreamExt;
use libp2p::core::either::EitherError;
use libp2p::multiaddr::Protocol;
use libp2p::ping::Failure;
use libp2p::swarm::{DummyBehaviour, SwarmEvent};
use libp2p::{development_transport, identity, Multiaddr, PeerId};
use libp2p::{ping, Swarm};
use libp2p::{rendezvous, NetworkBehaviour};
use void::Void;

const NAMESPACE: &str = "rendezvous";
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    tracing::info!("nice");

    let id = identity::Keypair::generate_ed25519();
    let rnd_point_addr = "/ip4/127.0.0.1/tcp/62649"
        .parse::<Multiaddr>()
        .expect("could not parse pt addr");

    let rnd_point = "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
        .parse::<PeerId>()
        .unwrap();

    let mut swarm = {
        let transport = development_transport(id.clone())
            .await
            .expect("transport error");
        let behaviour = MyBehaviour {
            rendezvous: rendezvous::client::Behaviour::new(id.clone()),
            ping: ping::Behaviour::new(ping::Config::new().with_interval(Duration::from_secs(1))),
            keep_alive: DummyBehaviour::default(),
        };
        let peer_id = PeerId::from(id.public());

        Swarm::new(transport, behaviour, peer_id)
    };

    tracing::info!("local peer id: {:?}", swarm.local_peer_id());

    swarm
        .dial(rnd_point_addr.clone())
        .expect("could not dial swarm");

    let mut cookie = None;
    let mut discover_tick = tokio::time::interval(Duration::from_secs(30));

    let match_event_fn =
        |event: SwarmEvent<MyEvent, EitherError<EitherError<Void, Failure>, Void>>| -> () {
            match event {
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    if peer_id == rnd_point {
                        tracing::info!(
                            "connected to rendezvous point, discovering nodes in {} namespace",
                            NAMESPACE
                        );

                        swarm.behaviour_mut().rendezvous.discover(
                            Some(rendezvous::Namespace::new(NAMESPACE.to_string()).unwrap()),
                            None,
                            None,
                            rnd_point,
                        );
                    }
                }
                SwarmEvent::Behaviour(my_event) => match my_event {
                    MyEvent::Rendezvous(rendezvous::client::Event::Discovered {
                        registrations,
                        cookie: new_cookie,
                        ..
                    }) => {
                        cookie.replace(new_cookie);

                        tracing::info!("registrations: {:#?}", registrations);
                        for registration in registrations {
                            for addr in registration.record.addresses() {
                                tracing::info!("addr: {:#?}", addr);
                                let peer = registration.record.peer_id();
                                tracing::info!("discovered peer {} at {}", peer, addr);

                                let p2p_suffix = Protocol::P2p(*peer.as_ref());
                                let addr_with_p2p = if !addr
                                    .ends_with(&Multiaddr::empty().with(p2p_suffix.clone()))
                                {
                                    addr.clone().with(p2p_suffix)
                                } else {
                                    addr.clone()
                                };

                                swarm
                                    .dial(addr_with_p2p)
                                    .expect("could not dial addr with p2p");
                            }
                        }
                    }
                    MyEvent::Ping(ping::Event {
                        peer,
                        result: Ok(ping::Success::Ping { rtt }),
                    }) if peer != rnd_point => {
                        tracing::info!("ping to {} is {}ms", peer, rtt.as_millis());
                    }

                    other => tracing::debug!("unhandled {:?}", other),
                },

                other => {
                    discover_tick.tick();
                    tracing::info!("unknown event : {:#?}", other);

                    swarm.behaviour_mut().rendezvous.discover(
                        Some(rendezvous::Namespace::new(NAMESPACE.to_string()).unwrap()),
                        cookie.clone(),
                        None,
                        rnd_point,
                    )
                }
            }
        };

    while let Some(event) = swarm.next().await {
        match event {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                if peer_id == rnd_point {
                    tracing::info!(
                        "connected to rendezvous point, discovering nodes in {} namespace",
                        NAMESPACE
                    );

                    swarm.behaviour_mut().rendezvous.discover(
                        Some(rendezvous::Namespace::new(NAMESPACE.to_string()).unwrap()),
                        None,
                        None,
                        rnd_point,
                    );
                }
            }
            SwarmEvent::Behaviour(my_event) => match my_event {
                MyEvent::Rendezvous(rendezvous::client::Event::Discovered {
                    registrations,
                    cookie: new_cookie,
                    ..
                }) => {
                    cookie.replace(new_cookie);

                    tracing::info!("registrations: {:#?}", registrations);
                    for registration in registrations {
                        for addr in registration.record.addresses() {
                            tracing::info!("addr: {:#?}", addr);
                            let peer = registration.record.peer_id();
                            tracing::info!("discovered peer {} at {}", peer, addr);

                            let p2p_suffix = Protocol::P2p(*peer.as_ref());
                            let addr_with_p2p =
                                if !addr.ends_with(&Multiaddr::empty().with(p2p_suffix.clone())) {
                                    addr.clone().with(p2p_suffix)
                                } else {
                                    addr.clone()
                                };

                            swarm
                                .dial(addr_with_p2p)
                                .expect("could not dial addr with p2p");
                        }
                    }
                }
                MyEvent::Ping(ping::Event {
                    peer,
                    result: Ok(ping::Success::Ping { rtt }),
                }) if peer != rnd_point => {
                    tracing::info!("ping to {} is {}ms", peer, rtt.as_millis());
                }

                other => tracing::debug!("unhandled {:?}", other),
            },

            other => {
                discover_tick.tick().await;
                tracing::info!("unknown event : {:#?}", other);

                swarm.behaviour_mut().rendezvous.discover(
                    Some(rendezvous::Namespace::new(NAMESPACE.to_string()).unwrap()),
                    cookie.clone(),
                    None,
                    rnd_point,
                )
            }
        }
    }
}

#[derive(Debug)]
enum MyEvent {
    Rendezvous(rendezvous::client::Event),
    Ping(ping::Event),
}

impl From<rendezvous::client::Event> for MyEvent {
    fn from(event: rendezvous::client::Event) -> Self {
        MyEvent::Rendezvous(event)
    }
}

impl From<ping::Event> for MyEvent {
    fn from(event: ping::Event) -> Self {
        MyEvent::Ping(event)
    }
}

impl From<Void> for MyEvent {
    fn from(event: Void) -> Self {
        void::unreachable(event)
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(event_process = false)]
#[behaviour(out_event = "MyEvent")]
struct MyBehaviour {
    rendezvous: rendezvous::client::Behaviour,
    ping: ping::Behaviour,
    keep_alive: DummyBehaviour,
}
