# This is an example of how you'd build the container image for your main application.

# This stage of the build checks out the proa source from GitHub and builds it.
FROM rust:1 AS proa-builder
WORKDIR /src
RUN git clone https://github.com/IronCoreLabs/proa .
RUN cargo build --release

# Substitute "your-application" here for the container image name that you would otherwise use.
# This stage augments that container image by adding proa.
FROM your-application as final
COPY --chown=0:0 --from=proa-builder /src/target/release/proa /bin/proa
