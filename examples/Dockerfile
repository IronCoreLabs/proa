# This is an example of how you'd build the container image for your main application.

# This stage of the build downloads proa from GitHub.
FROM alpine AS proa-builder
WORKDIR /src
ENV version="v0.1.1"
RUN wget -O proa https://github.com/IronCoreLabs/proa/releases/download/${version}/proa-x86_64-unknown-linux-gnu
RUN chmod +x proa

# Substitute "your-application" here for the container image name that you would otherwise use.
# This stage augments that container image by adding proa.
FROM your-application as final
COPY --chown=0:0 --from=proa-builder /src/target/release/proa /bin/proa
