use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "jcode-artifact-server",
    about = "Loopback artifact library browser"
)]
struct Cli {
    /// Address to bind. Defaults to loopback only.
    #[arg(long, default_value = "127.0.0.1:8789")]
    addr: SocketAddr,

    /// Artifact store root to browse.
    #[arg(long, default_value = ".jcode/artifacts")]
    store_root: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let server = jcode_artifact_server::ArtifactServer::new(cli.store_root);
    eprintln!("jcode artifact server listening on http://{}", cli.addr);
    let shutdown = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            eprintln!("artifact server shutdown signal failed: {error}");
        }
    };
    server
        .serve_until(cli.addr, shutdown)
        .await
        .with_context(|| format!("serving artifact catalog on {}", cli.addr))
}
