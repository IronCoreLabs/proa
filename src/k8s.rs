use std::time::Duration;

use anyhow::anyhow;
use futures::StreamExt;
use gethostname;
use k8s_openapi::api::core::v1::Pod;
use kube::{
    runtime::{
        watcher::{watcher, Config, Error},
        WatchStreamExt,
    },
    ResourceExt,
};
use kube::{Api, Client};
use tracing::{debug, debug_span, info};

use crate::stream::holistic_stream_ext::HolisticStreamExt;

// Kubernetes-related functions.

// Find the name of our own Pod, identify which container is ours, and watch all the other containers for readiness. Return when
// they're ready, or return an error.
#[tracing::instrument]
pub async fn wait_for_ready() -> Result<Pod, anyhow::Error> {
    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::default_namespaced(client);

    // Our Pod name is the same as our hostname.
    // TODO Strip domain parts off in case setHostnameAsFQDN is set.
    let myname = gethostname::gethostname();
    let myname = myname.into_string().unwrap();
    info!(myname, "Watching for Pod");

    let watch_config = format!("metadata.name={}", myname);
    let watch_config = Config::default().fields(watch_config.as_str());

    let events = watcher(pods, watch_config).applied_objects();
    // TODO: just an example of use, I know you don't want it here
    let ready_pods = events
        .filter_map(filter_ready)
        .holistic_timeout(Duration::new(10, 0));
    tokio::pin!(ready_pods);

    let ready_pod = ready_pods
        .next()
        .await
        .ok_or(anyhow!("Pod was never ready"))??;
    info!(myname, "Pod is ready");
    Ok(ready_pod)
}

// If error, log it.
// If all the pod's containers but this one are ready, return the pod.
// Else return None.
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

fn is_ready(pod: &Pod) -> Result<bool, anyhow::Error> {
    let span = debug_span!("is_ready");
    let _enter = span.enter();

    // The name of the main container in the Pod.
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
        .map(|s| {
            s.container_statuses.as_ref().map(|s| {
                s.iter()
                    .filter(|s| &s.name != main_cont_name)
                    .all(|s| s.started.unwrap_or(false) && s.ready)
            })
        })
        .flatten()
        .unwrap_or(false);
    debug!(status);
    Ok(*status)
}
