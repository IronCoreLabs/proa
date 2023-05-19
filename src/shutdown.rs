use anyhow::Error;
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use std::time::Duration;
use tracing::{debug, debug_span, info};

use crate::k8s;
use crate::stream::holistic_stream_ext::HolisticStreamExt;

pub async fn shutdown(pod: Pod) -> Result<(), Error> {
    let span = debug_span!("shutdown");
    let _enter = span.enter();

    // send_shutdown_reqs()?;
    wait_for_shutdown(pod).await?;

    Ok(())
}

// Log messages as the containers shut down.
// If the timeout expires, give up and log a message.
async fn wait_for_shutdown(pod: Pod) -> Result<(), Error> {
    let timeout: Option<i64> = pod
        .spec
        .and_then(|spec| spec.termination_grace_period_seconds);
    let timeout: u64 = match timeout {
        Some(x @ 0..) => x.try_into().unwrap(),
        _ => 30,
    };
    let timeout: Duration = Duration::new(timeout, 0);

    let events = k8s::watch_my_pod()
        .await?
        .holistic_timeout(timeout)
        .map(flatten_result)
        .inspect(log_progress)
        .filter_map(is_done);
    tokio::pin!(events);
    events.next().await;

    Ok(())
}

// Return a tuple of (running, total) to show how many of the pod's containers are still running.
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

// Use in filter_map to identify the last event in the stream. That's either when all the containers have terminated except one
// (which we assume is this one), or when an error occurs.
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

// Emit a log message indicating the progress we've made toward shutting down the containers in this pod.
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
        Err(err) => info!(?err),
    }
}

// Flatten a nested Result into a simple Result.
fn flatten_result<T, E1, E2, E3>(r: Result<Result<T, E1>, E2>) -> Result<T, E3>
where
    E1: Into<E3>,
    E2: Into<E3>,
{
    match r {
        Ok(Ok(t)) => Ok(t),
        Ok(Err(e1)) => Err(e1.into()),
        Err(e2) => Err(e2.into()),
    }
}
