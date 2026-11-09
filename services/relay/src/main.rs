use anyhow::Result;
use futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{identify, noise, ping, relay, tcp, yamux, Multiaddr, SwarmBuilder};
use std::net::Ipv4Addr;
use std::time::Duration;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[derive(NetworkBehaviour)]
struct Behaviour {
    relay: relay::Behaviour,
    ping: ping::Behaviour,
    identify: identify::Behaviour,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let port: u16 = std::env::var("RELAY_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4001);

    let keypair = Keypair::generate_ed25519();
    let peer_id = keypair.public().to_peer_id();

    let mut swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(|key| {
            Ok(Behaviour {
                relay: relay::Behaviour::new(
                    key.public().to_peer_id(),
                    relay::Config::default(),
                ),
                ping: ping::Behaviour::new(ping::Config::new()),
                identify: identify::Behaviour::new(identify::Config::new(
                    "mysync-relay/1.0.0".into(),
                    key.public(),
                )),
            })
        })?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    let listen_tcp: Multiaddr = Multiaddr::empty()
        .with(Protocol::Ip4(Ipv4Addr::UNSPECIFIED))
        .with(Protocol::Tcp(port));
    swarm.listen_on(listen_tcp)?;

    let listen_quic: Multiaddr = Multiaddr::empty()
        .with(Protocol::Ip4(Ipv4Addr::UNSPECIFIED))
        .with(Protocol::Udp(port))
        .with(Protocol::QuicV1);
    let _ = swarm.listen_on(listen_quic);

    info!("mysync relay peer id: {peer_id}");
    info!("clients should dial: /ip4/<PUBLIC_IP>/tcp/{port}/p2p/{peer_id}");

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("listening on {address}");
            }
            SwarmEvent::Behaviour(BehaviourEvent::Relay(event)) => {
                info!("relay event: {event:?}");
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                info!("connected: {peer_id}");
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                warn!("outgoing error to {peer_id:?}: {error}");
            }
            _ => {}
        }
    }
}
