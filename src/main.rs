use std::process::ExitCode;

use tracing::{info, debug, warn};

mod exec;
mod k8s;
mod stream;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt::init();
    info!("Starting up.");

    let status = inner_main().await;
    let status = match status {
        Ok(x) => x,
        Err(error) => {
            warn!(?error);
            1
        },
    };
    debug!(status, "Exiting.");
    status.into()
}

async fn inner_main() -> Result<u8, anyhow::Error> {
    k8s::wait_for_ready().await?;

    exec::run()
}
