mod session;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use peerglide_protocol::{Edge, Message, VERSION};
use peerglide_transport as transport;
use std::{net::SocketAddr, path::PathBuf, time::Duration};

#[derive(Parser)]
#[command(
    version,
    about = "PeerGlide — Multiple Computers. One Seamless Experience."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Create an identity. Exchange ONLY node.crt.der over an independently trusted channel.
    Init {
        #[arg(long, default_value = "identity")]
        identity: PathBuf,
    },
    /// Accept a connection from exactly the enrolled certificate. No input permission by default.
    Listen(Options),
    /// Establish a peer connection. Network roles do not determine input roles.
    Connect {
        address: SocketAddr,
        #[command(flatten)]
        options: Options,
        #[arg(long)]
        probe: bool,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}
impl From<Side> for Edge {
    fn from(s: Side) -> Self {
        match s {
            Side::Left => Self::Left,
            Side::Right => Self::Right,
            Side::Top => Self::Top,
            Side::Bottom => Self::Bottom,
        }
    }
}
#[derive(clap::Args)]
struct Options {
    #[arg(long, default_value = "identity")]
    identity: PathBuf,
    /// Explicitly enrolled remote public certificate; verify its BLAKE3 fingerprint out of band.
    #[arg(long)]
    peer_cert: PathBuf,
    #[arg(long, default_value = "0.0.0.0:24870")]
    bind: SocketAddr,
    /// Explicit consent to receive remote input (independent of connecting/listening).
    #[arg(long)]
    allow_control: bool,
    /// Enable real OS capture/injection; without this flag input is validated but not injected.
    #[arg(long)]
    native: bool,
    #[arg(long, value_enum, default_value = "right")]
    edge: Side,
}
#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Init { identity } => {
            let id = transport::Identity::create(&identity)?;
            println!(
                "Identity created. Public certificate: {}\nBLAKE3: {}\nKeep node.key.der private.",
                identity.join("node.crt.der").display(),
                id.fingerprint()
            );
            Ok(())
        }
        Command::Listen(o) => run(o, None, false).await,
        Command::Connect {
            address,
            options,
            probe,
        } => run(options, Some(address), probe).await,
    }
}
async fn run(o: Options, address: Option<SocketAddr>, probe: bool) -> Result<()> {
    let id = transport::Identity::load(&o.identity)?;
    let peer = std::fs::read(&o.peer_cert)?;
    anyhow::ensure!(
        peer != id.certificate,
        "a peer must have a distinct identity"
    );
    println!(
        "Local fingerprint: {}\nEnrolled peer: {}",
        id.fingerprint(),
        transport::fingerprint(&peer)
    );
    let e = transport::endpoint(o.bind, &id, &peer)?;
    println!(
        "Listening on {}. Incoming control: {}. Native input: {}.",
        e.local_addr()?,
        o.allow_control,
        o.native
    );
    let (c, mut tx, mut rx) = if let Some(addr) = address {
        let c = tokio::time::timeout(Duration::from_secs(5), e.connect(addr, "peerglide.local")?)
            .await??;
        transport::verify_peer(&c, &peer)?;
        let (tx, rx) = c.open_bi().await?;
        (c, tx, rx)
    } else {
        let incoming = tokio::select! { i=e.accept()=>i.context("endpoint closed")?,_=tokio::signal::ctrl_c()=>return Ok(()) };
        let c = tokio::time::timeout(Duration::from_secs(5), incoming).await??;
        transport::verify_peer(&c, &peer)?;
        let (tx, rx) = tokio::time::timeout(Duration::from_secs(5), c.accept_bi()).await??;
        (c, tx, rx)
    };
    transport::send(&mut tx, &Message::Hello { version: VERSION }).await?;
    let hello = tokio::time::timeout(Duration::from_secs(3), transport::receive(&mut rx)).await??;
    anyhow::ensure!(
        matches!(hello, Message::Hello { version: VERSION }),
        "unsupported protocol version"
    );
    println!("Authenticated peer connected. Ctrl+C exits; Ctrl+Alt+Esc restores local input.");
    if probe {
        transport::send(&mut tx, &Message::Ping { lease: 0 }).await?;
        anyhow::ensure!(
            matches!(
                tokio::time::timeout(Duration::from_secs(3), transport::receive(&mut rx)).await??,
                Message::Pong { lease: 0 }
            ),
            "probe response mismatch"
        );
        println!("PASS: mutual TLS, protocol negotiation, reliable roundtrip.");
        c.close(0u8.into(), b"probe complete");
        e.wait_idle().await;
        return Ok(());
    }
    let result = session::run(c.clone(), tx, rx, o.allow_control, o.native, o.edge.into()).await;
    c.close(0u8.into(), b"session ended");
    e.wait_idle().await;
    result
}
