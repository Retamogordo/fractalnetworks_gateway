FROM debian:11

ARG BUILD_TYPE=release
ENV RUST_LOG=info

# install dependencies
RUN apt update && \
    DEBIAN_FRONTEND=noninteractive apt install -y --no-install-recommends iptables iproute2 wireguard-tools nginx ca-certificates && \
    rm -rf /var/lib/apt/lists/*

RUN update-alternatives --set iptables /usr/sbin/iptables-legacy
RUN update-alternatives --set ip6tables /usr/sbin/ip6tables-legacy

# copy entrypoint and binary
COPY target/$BUILD_TYPE/fractal-gateway /usr/local/bin/fractal-gateway
COPY scripts/entrypoint.sh /bin/entrypoint.sh

ENTRYPOINT ["/bin/entrypoint.sh"]
