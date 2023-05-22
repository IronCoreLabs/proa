# Proa for Kubernetes sidecar management

Inspired by https://github.com/redboxllc/scuttle, https://github.com/joho/godotenv, and
https://github.com/kubernetes/enhancements/issues/753, among others.
This program is meant to be the entrypoint for the "main" container in a Pod that also contains sidecar containers. This program
is a wrapper around the main process in the main container. It waits for the sidecars to be ready before starting the main program,
and it shuts down the sidecars when the main process exits so the whole Pod can exit gracefully, as in the case of a Job.

Briefly, it does this:

1. Watch its own Pod's spec and status.
1. Wait until all containers (except its own) are ready.
1. Start the main process and wait for it to exit.
1. TODO Perform some shutdown actions, either the equivalent of `pkill` or hitting an HTTP endpoint on localhost.
1. Wait for the sidecars to exit.

Optional:
- TODO Serve a readiness HTTP endpoint to indicate when the main process is running.

# Usage

See [example.yaml](example.yaml) for a demonstration of how to use it. The Job has a sidecar, simulated by nginx, that must be
ready before the main process starts. We simulate a sidecar that starts slowly by making the nginx container run `sleep 30` first.
Proa uses Kubernetes' knowledge about the readiness of the sidecar container. That means the sidecars must each provide a
`readinessProbe`, and the Pod's `serviceAccount` needs permission to read and watch the Pod it's running in.

TODO Support https://github.com/cargo-bins/cargo-binstall. Explain how to build the container image.

## Requirements

- Sidecars need readinessProbes.
- Service account needs permission to read and watch its own Pod.
- Need to `shareProcessNamespace` so proa can stop the sidecars, and either
    - the main container with proa needs to run as UID 0 (not recommended)
    - all containers need to run as the same UID.

# Name

It's a program to manage sidecars, but sidecar is a motorcycle metaphor, and Kubernetes is all about nautical memes.
A [proa](https://en.wikipedia.org/wiki/Proa) is a sailboat with an outrigger, which is sort of like a sidecar on a motorcycle.

# Development

Requirements:
- Use [nix](https://github.com/NixOS/nix) and [direnv](https://github.com/direnv/direnv), or install the tools manually. See
    [flake.nix](flake.nix) for the list of tools you'll need.
- Docker or equivalent.

1. `kind create cluster` to start a tiny Kubernetes cluster in your local Docker.
1. `skaffold dev` to start the compile-build-deploy loop.

Every time you save a file, skaffold will rebuild and redeploy, then show you output from the containers in the Pod.
