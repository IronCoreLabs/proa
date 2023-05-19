use k8s_openapi::api::core::v1::Pod;
use tracing::debug_span;

pub fn shutdown(pod: Pod) -> Result<(), anyhow::Error> {
    let span = debug_span!("shutdown");
    let _enter = span.enter();

    kill_all()?;

    Ok(())
}

fn kill_all() -> Result<(), anyhow::Error> {
    // TODO Some kinda sync thing that's watching pod.status.container_statuses.state.terminated, intermixed with sending signals
    // based on Pod.terminationGracePeriodSeconds.
    // Send nice shutdown signals.
    // Monitor container status until they're all terminated. Use futures::stream::select.
    // Log messages as containers shut down.
    // If the timeout expires, give up and log a scary message.

}
