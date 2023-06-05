# Proa isn't distributed as a container image; this is an example of how you would use it in your own project. This image is also
# used for development.

FROM rust:1 AS chef
RUN cargo install cargo-chef
COPY rust-toolchain.toml /
RUN rustup show
# If there's an update to the rust toolchain that's newer than the cached layer from the above command, then the RUN step
# in the planner, below, will run slowly every time.
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder 
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build

# We do not need the Rust toolchain to run the binary!
FROM debian:bullseye-slim AS runtime
RUN apt-get update
RUN apt-get install libssl1.1
WORKDIR /app
COPY --from=builder /app/target/debug/proa /usr/local/bin
ENTRYPOINT ["/usr/local/bin/proa"]
