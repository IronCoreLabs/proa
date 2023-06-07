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

    let pod = k8s::wait_for_ready().await?;

    let status = exec::run(&cli.command, &cli.args);

    if let Err(err) = shutdown::shutdown(cli, pod).await {
        warn!(err = err.to_string(), "Shutdown problem");
    }

    info!(?status, "Exiting.");
    status.map(|c| c.into())
}
