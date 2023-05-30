use anyhow::Context;
use std::ffi::OsString;
use std::process::{Command, ExitStatus};
use tracing::{debug_span, info};

/// Run the main program. Pass its stdout and stderr through to the same places as ours. Capture its return status.
pub fn run(cmd: &OsString, args: &Vec<OsString>) -> Result<u8, anyhow::Error> {
    let span = debug_span!("run");
    let _enter = span.enter();

    // Build the command to run.
    let mut cmd = Command::new(cmd);
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
    let c = status.code();
    match c {
        Some(0) => 0,
        Some(n @ 1..=255) => n.try_into().unwrap(),
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;

    use super::*;

    #[test]
    fn run_exit_codes() -> Result<(), Error> {
        #[derive(Debug)]
        struct TestCase<'a> {
            name: &'a str,
            cmd: &'a str,
            args: Vec<&'a str>,
            stat: u8,
        }

        let tests = [
            TestCase {
                name: "simple",
                cmd: "true",
                args: vec![],
                stat: 0,
            },
            TestCase {
                name: "error",
                cmd: "false",
                args: vec![],
                stat: 1,
            },
            TestCase {
                name: "error 5",
                cmd: "sh",
                args: vec!["-c", "exit 5"],
                stat: 5,
            },
            TestCase {
                name: "non-u8 err",
                cmd: "sh",
                args: vec!["-c", "exit 257"],
                stat: 1,
            },
        ];

        for tc in tests {
            let args = tc.args.into_iter().map(|x| x.into()).collect();
            let exit_status = run(&tc.cmd.into(), &args)?;
            assert_eq!(exit_status, tc.stat, "{}", tc.name);
        }

        Ok(())
    }
}
