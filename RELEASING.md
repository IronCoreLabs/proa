1. Increment the version number in `Cargo.toml`.
1. `cargo fetch` to update `Cargo.lock`.
1. Commit and push.
1. Merge the PR.
1. `cargo publish`
1. Create a release on GitHub to trigger the `release.yaml` workflow.
