use anyhow::{anyhow, Error};
use futures::{Stream, StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    runtime::{
        watcher::{watcher, Config as KubeConfig},
        WatchStreamExt,
    },
    ResourceExt,
};
use kube::{Api, Client};
use tracing::{debug, debug_span, info};

// Kubernetes-related functions.

/// Find the name of our own Pod, identify which container is ours, and watch all the other containers for readiness. Return when
/// they're ready, or return an error.
#[tracing::instrument]
pub async fn wait_for_ready() -> Result<Pod, Error> {
    let events = watch_my_pod().await?;
    let ready_pods = events.filter_map(filter_ready);
    let mut ready_pods = Box::pin(ready_pods);

    let ready_pod = ready_pods
        .next()
        .await
        .ok_or(anyhow!("Pod was never ready"))?;
    info!("Pod is ready");
    Ok(ready_pod)
}

/// Return a stream providing Pod events about the pod we're running in.
pub async fn watch_my_pod() -> Result<impl Stream<Item = Result<Pod, Error>>, Error> {
    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::default_namespaced(client);

    // Our Pod name is the same as our hostname.
    let myname = gethostname::gethostname();
    let myname = myname.into_string().unwrap();
    // Strip domain parts off in case setHostnameAsFQDN is set.
    let myname = myname.split('.').next().unwrap();
    info!(myname, "Watching for Pod");

    let watch_config = format!("metadata.name={}", myname);
    let watch_config = KubeConfig::default().fields(watch_config.as_str());

    let pods = watcher(pods, watch_config).applied_objects();
    let pods = pods.map_err(|e| anyhow!(e));
    Ok(pods)
}

/// If error, log it.
/// If all the pod's containers but this one are ready, return the pod.
/// Else return None.
async fn filter_ready(pod: Result<Pod, Error>) -> Option<Pod> {
    match pod {
        Err(e) => {
            info!("Watch error: {}", e);
            None
        }
        Ok(p) => {
            debug!("Saw Pod {}...", p.name_any());
            is_ready(&p).map_or_else(
                |e| {
                    info!("Unsure if ready: {}", e);
                    None
                },
                |ready| if ready { Some(p) } else { None },
            )
        }
    }
}

/// Return true if this Pod is ready for the main process to start. That means all the containers except the main one are signaling
/// ready status.
fn is_ready(pod: &Pod) -> Result<bool, Error> {
    let span = debug_span!("is_ready");
    let _enter = span.enter();

    // The name of the main container in the Pod. For now we pick containers[0].
    let main_cont_name = &pod
        .spec
        .as_ref()
        .ok_or(anyhow!("No pod.spec"))?
        .containers
        .get(0)
        .ok_or(anyhow!("No pod.spec.containers[0]"))?
        .name;

    let status = &pod
        .status
        .as_ref()
        .and_then(|s| {
            s.container_statuses.as_ref().map(|s| {
                s.iter()
                    .filter(|s| &s.name != main_cont_name)
                    .all(|s| s.ready)
            })
        })
        .unwrap_or(false);
    debug!(status);
    Ok(*status)
}

#[cfg(test)]
mod tests {
    use json::object;

    use super::*;

    #[tokio::test]
    async fn check_ready() -> Result<(), Error> {
        // Pass in an error, it's not ready.
        assert_eq!(filter_ready(Err(anyhow!["foo"])).await, None);

        // A pod where only the main container is ready.
        let pod = object! {
            apiVersion: "v1",
            kind: "Pod",
            metadata: { name: "pod1" },
            spec: {
                containers: [{ name: "cont1" }]
            },
            status: {
                containerStatuses: [{ name: "cont1", ready: true }]
            }
        };
        let pod: Pod = serde_json::from_str(pod.dump().as_str())?;
        assert_eq!(filter_ready(Ok(pod.clone())).await, Some(pod));

        // A pod with only the main container, which isn't ready.
        let pod = object! {
            apiVersion: "v1",
            kind: "Pod",
            metadata: { name: "pod1" },
            spec: {
                containers: [{ name: "cont1" }]
            },
            status: {
                containerStatuses: [{ name: "cont1", ready: false }]
            }
        };
        let pod: Pod = serde_json::from_str(pod.dump().as_str())?;
        assert_eq!(filter_ready(Ok(pod.clone())).await, Some(pod));

        // A pod with one ready sidecar.
        let pod = object! {
            apiVersion: "v1",
            kind: "Pod",
            metadata: { name: "pod1" },
            spec: {
                containers: [
                    { name: "cont1" },
                    { name: "cont2" },
                ]
            },
            status: {
                containerStatuses: [
                    { name: "cont1", ready: true },
                    { name: "cont2", ready: true },
                ]
            }
        };
        let pod: Pod = serde_json::from_str(pod.dump().as_str())?;
        assert_eq!(filter_ready(Ok(pod.clone())).await, Some(pod));

        // A pod with one not-ready sidecar.
        let pod = object! {
            apiVersion: "v1",
            kind: "Pod",
            metadata: { name: "pod1" },
            spec: {
                containers: [
                    { name: "cont1" },
                    { name: "cont2" },
                ]
            },
            status: {
                containerStatuses: [
                    { name: "cont1", ready: true },
                    { name: "cont2", ready: false },
                ]
            }
        };
        let pod: Pod = serde_json::from_str(pod.dump().as_str())?;
        assert_eq!(filter_ready(Ok(pod.clone())).await, None);

        // A pod with one ready sidecar, one not-ready.
        let pod = object! {
            apiVersion: "v1",
            kind: "Pod",
            metadata: { name: "pod1" },
            spec: {
                containers: [
                    { name: "cont1" },
                    { name: "cont2" },
                    { name: "cont3" },
                ]
            },
            status: {
                containerStatuses: [
                    { name: "cont1", ready: true },
                    { name: "cont2", ready: true },
                    { name: "cont3", ready: false },
                ]
            }
        };
        let pod: Pod = serde_json::from_str(pod.dump().as_str())?;
        assert_eq!(filter_ready(Ok(pod.clone())).await, None);

        // A pod with two ready sidecars.
        let pod = object! {
            apiVersion: "v1",
            kind: "Pod",
            metadata: { name: "pod1" },
            spec: {
                containers: [
                    { name: "cont1" },
                    { name: "cont2" },
                    { name: "cont3" },
                ]
            },
            status: {
                containerStatuses: [
                    { name: "cont1", ready: true },
                    { name: "cont2", ready: true },
                    { name: "cont3", ready: true },
                ]
            }
        };
        let pod: Pod = serde_json::from_str(pod.dump().as_str())?;
        assert_eq!(filter_ready(Ok(pod.clone())).await, Some(pod));

        Ok(())
    }
}
