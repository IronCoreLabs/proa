use anyhow::Error;
use clap::{crate_name, crate_version};
use futures::future::join_all;
use futures::{Future, FutureExt, StreamExt};
use k8s_openapi::api::core::v1::Pod;
use reqwest::Client;
use reqwest::{Method, Url};
use std::time::Duration;
use tracing::{debug, debug_span, info, warn};

use crate::config::Cli;
use crate::k8s;
use crate::stream::holistic_stream_ext::HolisticStreamExt;

/// Shut down the sidecars and wait for them to terminate.
pub async fn shutdown(cli: Cli, pod: Pod) -> Result<(), Error> {
    let span = debug_span!("shutdown");
    let _enter = span.enter();

    info!("Sending shutdown requests.");

    send_shutdown_reqs(cli).await;
    wait_for_shutdown(pod).await?;

    Ok(())
}

/// Send requests for all the other containers in the Pod to shut down.
async fn send_shutdown_reqs(cli: Cli) {
    #[cfg(feature = "kill")]
    send_shutdown_with_kill(cli).await;
    #[cfg(not(feature = "kill"))]
    send_shutdown_normal(&cli).await;
}

#[cfg(feature = "kill")]
async fn send_shutdown_with_kill(cli: Cli) {
    let do_nothing = cli.shutdown_http_get.is_empty()
        && cli.shutdown_http_post.is_empty()
        && cli.kill.is_empty();

    send_shutdown_normal(&cli).await;

    cli.kill.into_iter().for_each(kill::kill_by_name);

    // If given no explicit shutdown instructions, just kill everything.
    if do_nothing {
        kill::kill_all();
    }
}

async fn send_shutdown_normal(cli: &Cli) {
    let user_agent = format!("{} v{}", crate_name!(), crate_version!());
    let client = Client::builder().user_agent(user_agent).build();
    match client {
        Err(err) => warn!(
            err = err.to_string(),
            "Unable to build HTTP client; no HTTP shutdown requests will be sent."
        ),
        Ok(client) => send_http_shutdowns(&cli, &client).await,
    }
}

fn send_http_shutdowns(cli: &Cli, client: &Client) -> impl Future<Output = ()> {
    let msgs = cli
        .shutdown_http_get
        .iter()
        .map(|url| send_http(client, url.clone(), Method::GET));
    let msgs = msgs.chain(
        cli.shutdown_http_post
            .iter()
            .map(|url| send_http(client, url.clone(), Method::POST)),
    );
    join_all(msgs).map(|_| ())
}

/// Send an HTTP request. If it fails, log the failure.
fn send_http(client: &Client, url: Url, method: Method) -> impl Future<Output = ()> {
    let req = client.request(method.clone(), url.clone());
    let resp = req.send();
    resp.map(|r: Result<_, _>| match r {
        Ok(x) => x.error_for_status(),
        Err(e) => Err(e),
    })
    .map(|r: Result<_, _>| r.err())
    .then(|x: Option<reqwest::Error>| async move {
        x.into_iter().for_each(|err| {
            warn!(
                err = err.to_string(),
                url = url.to_string(),
                ?method,
                "Error sending shutdown request"
            )
        })
    })
}

#[cfg(feature = "kill")]
mod kill {
    use nix::{
        sys::signal::{self, Signal},
        unistd,
    };
    use std::ffi::{OsStr, OsString};
    use sysinfo::{Pid, PidExt, Process, ProcessExt, System, SystemExt};
    use tracing::{debug, info, trace};

    /// Send a TERM signal to every process that we can see, except our own.
    #[tracing::instrument]
    pub fn kill_all() {
        debug!("Killing all visible processes.");
        let mut sys = System::new();
        sys.refresh_processes();
        sys.processes()
            .into_iter()
            .filter(|&(_pid, process)| process.exe().file_name() != Some(OsStr::new("proa")))
            .for_each(|(pid, proc)| kill_one(pid, proc));
    }

    /// Find any processes running the named executable, and terminate them.
    pub fn kill_by_name(pname: OsString) {
        // It's inefficient to create and refresh sys each time this function is called.
        let mut sys = System::new();
        sys.refresh_processes();
        sys.processes()
            .into_iter()
            .filter(|&(_pid, process)| process.exe().file_name() == Some(&pname))
            .for_each(|(pid, proc)| kill_one(pid, proc));
    }

    /// Terminate one process by PID. Process is used for log messages.
    fn kill_one(pid: &Pid, process: &Process) {
        trace!("Killing PID {} ({})", pid, process.name());
        let pid = pid.as_u32();
        let pid = unistd::Pid::from_raw(pid.try_into().unwrap());
        signal::kill(pid, Signal::SIGTERM)
            .err()
            .into_iter()
            .for_each(|err| {
                info!(
                    err = err.desc(),
                    "Unable to kill PID {} ({})",
                    pid,
                    process.name()
                );
            });
    }
}

/// Log messages as the containers shut down.
/// If the timeout expires, give up and log a message.
async fn wait_for_shutdown(pod: Pod) -> Result<(), Error> {
    let timeout: Option<i64> = pod
        .spec
        .and_then(|spec| spec.termination_grace_period_seconds);
    let timeout: u64 = match timeout {
        Some(x @ 0..) => x.try_into().unwrap(),
        _ => {
            debug!("Defaulting to 30 seconds");
            30
        }
    };
    let timeout: Duration = Duration::new(timeout, 0);

    let events = k8s::watch_my_pod()
        .await?
        .holistic_timeout(timeout)
        .map(flatten_result)
        .inspect(log_progress)
        .filter_map(is_done);
    tokio::pin!(events);
    if let Some(Err(err)) = events.next().await {
        info!(err = err.to_string(), "Error waiting for sidecars to exit");
    }

    Ok(())
}

/// Use in filter_map to identify the last event in the stream. That's either when all the containers have terminated except one
/// (which we assume is this one), or when an error occurs.
// We can't just use .status.phase, because that indicates the status of the entire Pod, and we're micro-managing based on statuses
// of individual conatiners.
async fn is_done(maybe_pod: Result<Pod, Error>) -> Option<Result<Pod, Error>> {
    match maybe_pod {
        Ok(pod) => {
            let (running, _) = pod_status(pod.clone());
            if running == Some(1) {
                Some(Ok(pod))
            } else {
                None
            }
        }
        Err(e) => Some(Err(e)),
    }
}

/// Emit a log message indicating the progress we've made toward shutting down the containers in this pod.
fn log_progress(maybe_pod: &Result<Pod, Error>) {
    fn fmt_or_unknown(n: Option<usize>) -> String {
        n.map_or("<unknown>".to_string(), |n| format!("{}", n))
    }

    match maybe_pod {
        Ok(pod) => {
            let (running, total) = pod_status(pod.clone());
            let running = fmt_or_unknown(running);
            let total = fmt_or_unknown(total);
            debug!("{}/{} containers are still running.", running, total)
        }
        Err(err) => info!(err = err.to_string()),
    }
}

/// Return a tuple of (running, total) to show how many of the pod's containers are still running.
fn pod_status(pod: Pod) -> (Option<usize>, Option<usize>) {
    // How many containers are still running?
    let running: Option<usize> = pod
        .status
        .and_then(|pod_status| pod_status.container_statuses)
        .map(|c_statuses| {
            c_statuses
                .into_iter()
                .flat_map(|c_status| c_status.state.and_then(|c_state| c_state.running))
                .count()
        });

    // How many containers are there total?
    let total: Option<usize> = pod.spec.map(|s| s.containers.len());

    (running, total)
}

/// Flatten a nested Result into a simple Result.
// https://github.com/rust-lang/rust/issues/70142
fn flatten_result<T, E1, E2, E>(r: Result<Result<T, E1>, E2>) -> Result<T, E>
where
    E: From<E1>,
    E: From<E2>,
{
    Ok(r??)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use json::object;

    #[tokio::test]
    async fn test_is_done() -> Result<(), Error> {
        // An error should be returned.
        let result = Err(anyhow!("oops"));
        let done = is_done(result).await;
        assert!(done.is_some());

        // A pod with one running container.
        let pod = object! {
            apiVersion: "v1",
            kind: "Pod",
            metadata: { name: "pod1" },
            spec: {
                containers: [{ name: "cont1" }]
            },
            status: {
                containerStatuses: [ { name: "cont1", state: { running: { startedAt: "2020-02-02T20:20:02Z" } } } ]
            }
        };
        let pod: Pod = serde_json::from_str(pod.dump().as_str())?;
        let result = Ok(pod);
        let done = is_done(result).await;
        assert!(done.is_some());

        // A pod with two running containers.
        let pod = object! {
            apiVersion: "v1",
            kind: "Pod",
            metadata: { name: "pod1" },
            spec: {
                containers: [
                    { name: "cont1" },
                    { name: "cont2" }
                ]
            },
            status: {
                containerStatuses: [ { name: "cont1", state: { running: { startedAt: "2020-02-02T20:20:02Z" } } } ],
                containerStatuses: [ { name: "cont2", state: { running: { startedAt: "2020-02-20T02:02:20Z" } } } ]
            }
        };
        let pod: Pod = serde_json::from_str(pod.dump().as_str())?;
        let result = Ok(pod);
        let done = is_done(result).await;
        assert!(done.is_some());

        // A pod with two containers, one running and one stopped.
        let pod = object! {
            apiVersion: "v1",
            kind: "Pod",
            metadata: { name: "pod1" },
            spec: {
                containers: [
                    { name: "cont1" },
                    { name: "cont2" }
                ]
            },
            status: {
                containerStatuses: [ { name: "cont1", state: { running: { startedAt: "2020-02-02T20:20:02Z" } } } ],
                containerStatuses: [ { name: "cont2", state: { terminated: { finishedAt: "2020-02-20T02:02:20Z" } } } ]
            }
        };
        let pod: Pod = serde_json::from_str(pod.dump().as_str())?;
        let result = Ok(pod);
        let done = is_done(result).await;
        assert!(done.is_none());

        Ok(())
    }
}
