FROM ubuntu:20.04

ARG BUILD_TYPE=release
ENV GATEWAY_TOKEN=abc
ENV GATEWAY_MANAGER=https://api.staging.fractalnetworks.co/manager/us/gateway
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1

# install dependencies
RUN apt update && \
    DEBIAN_FRONTEND=noninteractive apt install -y --no-install-recommends iptables iproute2 wireguard-tools nginx ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# copy entrypoint and binary
COPY target/$BUILD_TYPE/fractal-gateway /usr/local/bin/fractal-gateway
COPY scripts/entrypoint.sh /bin/entrypoint.sh

ENTRYPOINT ["/bin/entrypoint.sh"]
