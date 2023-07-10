use std::process::ExitCode;

use anyhow::Error;
use clap::Parser;
use config::Cli;
use tracing::{info, warn};

mod config;
mod exec;
mod k8s;
mod shutdown;
mod stream;

#[tokio::main]
async fn main() -> Result<ExitCode, Error> {
    let cli = Cli::parse();

    tracing_subscriber::fmt().json().init();
    info!("Starting up.");

    let wait_result = k8s::wait_for_ready().await;

    // If sidecar startup was successful, then keep a copy of our Pod for later, and also run the wrapped program.
    let (maybe_pod, status) = match wait_result {
        Ok(_) => (wait_result.ok(), exec::run(&cli.command, &cli.args)),
        Err(e) => (None, Err(e)),
    };

    if let Err(err) = shutdown::shutdown(cli, maybe_pod).await {
        warn!(err = err.to_string(), "Shutdown problem");
    }

    info!(?status, "Exiting.");
    status.map(|c| c.into())
}
