FROM debian:testing

ARG BUILD_TYPE=release
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1

# install dependencies
RUN apt update && \
    DEBIAN_FRONTEND=noninteractive apt install -y --no-install-recommends iproute2 wireguard-tools ca-certificates iputils-ping && \
    rm -rf /var/lib/apt/lists/*

# copy entrypoint and binary
COPY target/$BUILD_TYPE/fractal-gateway-integration /usr/local/bin/fractal-gateway-integration

ENTRYPOINT ["/usr/local/bin/fractal-gateway-integration"]

