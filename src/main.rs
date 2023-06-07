use std::process::ExitCode;

use clap::Parser;
use config::Cli;
use tracing::{info, warn};

mod config;
mod exec;
mod k8s;
mod shutdown;
mod stream;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    tracing_subscriber::fmt().json().init();
    info!("Starting up.");

    let status = inner_main(cli).await.unwrap_or_else(|e| { 
        warn!(error = error.to_string());
        1
    });
    info!(status, "Exiting.");
    status.into()
}

/// Convenience function so we can return a Result.
async fn inner_main(cli: Cli) -> Result<u8, anyhow::Error> {
    let pod = k8s::wait_for_ready().await?;

    let result = exec::run(&cli.command, &cli.args);

    if let Err(err) = shutdown::shutdown(cli, pod).await {
        warn!(err = err.to_string(), "Shutdown problem");
    }

    result
}
