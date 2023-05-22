use anyhow::Context;
use std::env;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus};
use tracing::{debug_span, info};

/// Run the main program. Pass its stdout and stderr through to the same places as ours. Capture its return status.
pub fn run() -> Result<u8, anyhow::Error> {
    let span = debug_span!("run");
    let _enter = span.enter();

    // Build the command to run.
    let mut args = env::args();
    let _discard = args.next();
    let mut cmd = Command::new(args.next().unwrap());
    let cmd = cmd.args(args);

    // Run it and return the status.
    info!(?cmd, "Running");
    let status = cmd.status().with_context(|| {
        format!(
            "Failed to execute {:?} {:?}",
            cmd.get_program(),
            cmd.get_args()
        )
    })?;

    info!(?cmd, ?status, "Done running");
    let status = exit_code(status);
    Ok(status)
}

/// Convert ExitStatus to a u8 that we can use as our own exit status.
fn exit_code(status: ExitStatus) -> u8 {
    let c = status.into_raw();
    match c {
        0 => 0,
        1..=255 => c.try_into().unwrap(),
        _ => 1,
    }
}
