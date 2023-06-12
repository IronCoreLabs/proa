# Proa for Kubernetes sidecar management

[![crates.io](https://img.shields.io/crates/v/proa.svg)](https://crates.io/crates/proa)
[![CI status](https://github.com/IronCoreLabs/proa/actions/workflows/rust-ci.yaml/badge.svg)](https://github.com/IronCoreLabs/proa/actions)
[![Rust Report Card](https://rust-reportcard.xuri.me/badge/github.com/ironcorelabs/proa)](https://rust-reportcard.xuri.me/report/github.com/ironcorelabs/proa)

Inspired by https://github.com/redboxllc/scuttle, https://github.com/joho/godotenv, and
https://github.com/kubernetes/enhancements/issues/753, among others.
This program is meant to be the entrypoint for the "main" container in a Pod that also contains sidecar containers. Proa
is a wrapper around the main process in the main container. It waits for the sidecars to be ready before starting the main program,
and it shuts down the sidecars when the main process exits so the whole Pod can exit gracefully, as in the case of a
[Job](https://kubernetes.io/docs/concepts/workloads/controllers/job/#handling-pod-and-container-failures).

![Drawing of a proa](Proa.png)

Briefly, it does this:

1. Watch its own Pod's spec and status.
1. Wait until all containers (except its own) are ready.
1. Start the main (wrapped) process and wait for it to exit.
1. Perform some shutdown actions, hitting an HTTP endpoint on localhost or sending signals like `pkill` would.
1. Wait for the sidecars to exit.

If it encounters errors during shutdown, it logs each error, but it exits with the same exit code as the wrapped process.

## Requirements

- Sidecars need readinessProbes.
- Service account needs permission to read and watch its own Pod.

## Usage

If you like, just copy [job.yaml](examples/job.yaml) and modify it for your use. The Job has a sidecar, simulated by a Python
script, that must be ready before the main process starts. We simulate a sidecar that starts slowly by sleeping for 30 seconds
before starting the Python HTTP server. Proa uses Kubernetes' knowledge about the readiness of the sidecar container. That means
the sidecars must each provide a `readinessProbe`, and the Pod's `serviceAccount` needs permission to read and watch the Pod it's
running in.

Or if you prefer, follow this step-by-step guide:
1. Build a container image that has both your main application and the `proa` executable. The easiest way to do this is probably
    to use a multi-stage Dockerfile to compile `proa` and `COPY` it into your final image. See [Dockerfile](examples/Dockerfile)
    for an example.
1. Create a `ServiceAccount` for your Job to use.
1. Create a `Role` and `RoleBinding` giving the service account permission to `get`, `watch`, and `list` the `pods` in its own
    namespace.
1. Modify the Job `spec.template.spec.serviceAccountName` to refer to that service account.
1. Modify the Job and ensure that the `spec.template.spec.containers` entry for every sidecar has a `readinessProbe`. (It doesn't
    matter if the main container has a readiness probe; proa will ignore it.)
1. Change the entrypoint (`command` and/or `args`) of the main container to call proa.
    - Pass flags to tell proa how to shut down your sidecars. This will usually be `--shutdown-http-get=URL` or
        `--shutdown-http-post=URL`. Those flags can be repeated multiple times.
    - Pass the separator string `--`, followed by the path to the main program and all its arguments.
1. Optionally add a `RUST_LOG` environment variable to the main container to control proa's logging verbosity.

## Killing

When it's time to shut down, proa can end the processes in your sidecars by sending SIGTERM, but it's probably not what you want.
Most processes that receive SIGTERM will exit with status 143, or some other nonzero value. Kubernetes will interpret that as a
container failure, and it will restart or recreate the Pod to try again.

If you're sure you want to use this, compile the program with feature `kill`, and also make sure your Pod meets these requirements:
- Need to `shareProcessNamespace` so proa can stop the sidecars, and either
    - the main container with proa needs to run as UID 0 (not recommended)
    - all containers need to run as the same UID.
- Don't use `hostPID`, or chaos will result as it tries to kill every process on the node.

## Name

It's a program to manage sidecars, but sidecar is a motorcycle metaphor, and Kubernetes is all about nautical memes.
A [proa](https://en.wikipedia.org/wiki/Proa) is a sailboat with an outrigger, which is sort of like a sidecar on a motorcycle.

## Development

Requirements:
- Use [nix](https://github.com/NixOS/nix) and [direnv](https://github.com/direnv/direnv), or install the tools manually. See
    [flake.nix](flake.nix) for the list of tools you'll need.
- Docker or equivalent.

1. `kind create cluster` to start a tiny Kubernetes cluster in your local Docker.
1. `skaffold dev` to start the compile-build-deploy loop.

Every time you save a file, skaffold will rebuild and redeploy, then show you output from the containers in the Pod.


---

## Sponsors

This project sponsored by [IronCore Labs](https://ironcorelabs.com/), makers of data privacy and security solutions for cloud apps including [SaaS Shield](https://ironcorelabs.com/products/saas-shield/) for multi-tenant SaaS apps and [Cloaked Search](https://ironcorelabs.com/products/cloaked-search/) for data-in-use encryption over Elasticsearch and OpenSearch. 
