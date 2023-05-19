use anyhow::{anyhow, Error};
use futures::stream::{once, select};
use futures::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, debug_span, info};

use crate::k8s;

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
    let timeout = sleep(timeout);
    let timeout = once(timeout);
    let timeout = timeout.map(|_t| Err(anyhow!("timeout expired")));

    let events = k8s::watch_my_pod().await?;

    let timed_events = select(events, timeout);
    let timed_events = timed_events.inspect(log_progress);
    let timed_events = timed_events.filter_map(is_done);
    tokio::pin!(timed_events);
    timed_events.next().await;

    Ok(())
}

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
