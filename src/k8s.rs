use anyhow::{anyhow, Error};
use futures::{Stream, StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    runtime::{
        watcher::{default_backoff, watch_object},
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
    info!(err = ready_pod.is_err(), "Done waiting for Pod.");
    ready_pod
}

/// Return a stream providing Pod events about the pod we're running in.
pub async fn watch_my_pod() -> Result<impl Stream<Item = Result<Option<Pod>, Error>>, Error> {
    let client = Client::try_default().await?;
    let pods_api: Api<Pod> = Api::default_namespaced(client);

    // Our Pod name is the same as our hostname.
    let myname = gethostname::gethostname();
    let myname = myname.into_string().unwrap();
    // Strip domain parts off in case setHostnameAsFQDN is set.
    let myname = myname.split('.').next().unwrap();
    info!(myname, "Watching for Pod");

    let pod = watch_object(pods_api, myname)
        .backoff(default_backoff())
        .map_err(|e| anyhow!(e));
    Ok(pod)
}

/// If we're done waiting for readiness, return something: either the ready Pod or an error.
/// If we're not done waiting, return None.
async fn filter_ready(pod: Result<Option<Pod>, Error>) -> Option<Result<Pod, Error>> {
    match pod {
        Err(e) => {
            info!("Watch error: {}", e);
            None
        }
        Ok(None) => {
            debug!("Pod was deleted?");
            None
        }
        Ok(Some(p)) => {
            debug!("Saw Pod {}...", p.name_any());
            match is_ready(&p) {
                // Keep waiting for readiness.
                WatchResult::NotReady => None,
                // If we see a k8s API error, log it and keep waiting.
                WatchResult::ApiError(e) => {
                    info!("Unsure if ready: {}", e);
                    None
                }
                // If all the sidecars are ready, return the Pod.
                WatchResult::Ready => Some(Ok(p)),
                // One of the sidecars terminated.
                WatchResult::PodError(e) => {
                    if p.spec
                        .map(|s| s.restart_policy == Some("Never".to_string()))
                        .unwrap_or(true)
                    {
                        // If restartPolicy == Never, then return an error because there's no point in waiting.
                        Some(Err(e))
                    } else {
                        // Any other restartPolicy means k8s will restart the sidecar; we should keep waiting for readiness.
                        None
                    }
                }
            }
        }
    }
}

/// The result of watching a Pod.
enum WatchResult {
    /// The Pod isn't ready yet.
    NotReady,
    /// The Pod is ready to execute the main program.
    Ready,
    /// Encountered a k8s API error while watching the Pod.
    ApiError(Error),
    /// The Pod (probably one of it containers) experienced an error.
    PodError(Error),
}

/// Return true if this Pod is ready for the main process to start. That means all the containers except the main one are signaling
/// ready status.
fn is_ready(pod: &Pod) -> WatchResult {
    let span = debug_span!("is_ready");
    let _enter = span.enter();

    // The name of the main container in the Pod. For now we pick containers[0].
    let main_cont_name = match main_cont_name(&pod) {
        Ok(name) => name,
        Err(e) => return WatchResult::ApiError(e),
    };

    // Are all of the sidecar containers ready?
    let ready = &pod
        .status
        .as_ref()
        .and_then(|s| {
            s.container_statuses.as_ref().map(|s| {
                s.iter()
                    .filter(|s| s.name != main_cont_name)
                    .all(|s| s.ready)
            })
        })
        .unwrap_or(false);
    // Are any of the sidecar containers terminated?
    let error = &pod.status.as_ref().and_then(|pod_stat| {
        pod_stat.container_statuses.as_ref().map(|cont_stats| {
            cont_stats
                .iter()
                .filter(|cont_stat| cont_stat.name != main_cont_name)
                .any(|cont_stat| {
                    cont_stat
                        .state
                        .as_ref()
                        .map(|state| {
                            if state.terminated.is_some() {
                                debug!(container = cont_stat.name, "Sidecar container terminated");
                                true
                            } else {
                                false
                            }
                        })
                        .unwrap_or(false)
                })
        })
    });
    debug!(ready, error);
    match (error, ready) {
        (Some(true), _) => {
            WatchResult::PodError(anyhow!("A sidecar container terminated prematurely"))
        }
        (_, false) => WatchResult::NotReady,
        (_, true) => WatchResult::Ready,
    }
}

fn main_cont_name(pod: &Pod) -> Result<String, Error> {
    Ok(pod
        .spec
        .as_ref()
        .ok_or(anyhow!("No pod.spec"))?
        .containers
        .get(0)
        .ok_or(anyhow!("No pod.spec.containers[0]"))?
        .name
        .clone())
}

#[cfg(test)]
mod tests {
    use json::object;

    use super::*;

    #[tokio::test]
    async fn check_ready() -> Result<(), Error> {
        // Pass in an error, it's not ready.
        assert!(filter_ready(Err(anyhow!["foo"])).await.is_none());

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
        assert_eq!(
            filter_ready(Ok(Some(pod.clone()))).await.unwrap().unwrap(),
            pod
        );

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
        assert_eq!(
            filter_ready(Ok(Some(pod.clone()))).await.unwrap().unwrap(),
            pod
        );

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
        assert_eq!(
            filter_ready(Ok(Some(pod.clone()))).await.unwrap().unwrap(),
            pod
        );

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
        assert!(filter_ready(Ok(Some(pod.clone()))).await.is_none());

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
        assert!(filter_ready(Ok(Some(pod.clone()))).await.is_none());

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
        assert_eq!(
            filter_ready(Ok(Some(pod.clone()))).await.unwrap().unwrap(),
            pod
        );

        // A pod with a sidecar that failed and won't be restarted.
        let pod = object! {
            apiVersion: "v1",
            kind: "Pod",
            metadata: { name: "pod1" },
            spec: {
                containers: [
                    { name: "cont1" },
                    { name: "cont2" },
                ],
                restartPolicy: "Never"
            },
            status: {
                containerStatuses: [
                    { name: "cont1", ready: true },
                    { name: "cont2", state: { terminated: { exitCode: 1 } }  },
                ]
            }
        };
        let pod: Pod = serde_json::from_str(pod.dump().as_str())?;
        assert!(filter_ready(Ok(Some(pod.clone()))).await.unwrap().is_err());

        // A pod with a sidecar that failed and will be restarted.
        let pod = object! {
            apiVersion: "v1",
            kind: "Pod",
            metadata: { name: "pod1" },
            spec: {
                containers: [
                    { name: "cont1" },
                    { name: "cont2" },
                ],
            },
            status: {
                containerStatuses: [
                    { name: "cont1", ready: true },
                    { name: "cont2", state: { terminated: { exitCode: 1 } }  },
                ]
            }
        };
        let pod: Pod = serde_json::from_str(pod.dump().as_str())?;
        assert!(filter_ready(Ok(Some(pod.clone()))).await.is_none());

        Ok(())
    }
}
