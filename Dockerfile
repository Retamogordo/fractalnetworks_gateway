FROM ubuntu:20.04

ENV GATEWAY_REST_PORT=8000
ENV GATEWAY_REST_ADDR=0.0.0.0
ENV GATEWAY_DATABASE=/tmp/gateway.db
ENV GATEWAY_TOKEN=abc

ENV ROCKET_ADDRESS=${GATEWAY_REST_ADDR}
ENV ROCKET_PORT=${GATEWAY_REST_PORT}
ENV RUST_LOG=info,sqlx=warn
ENV RUST_BACKTRACE=1

RUN apt update && apt install -y --no-install-recommends iptables iproute2 wireguard-tools nginx && rm -rf /var/lib/apt/lists/*
COPY /target/release/gateway /usr/local/bin/gateway
COPY scripts/entrypoint.sh /bin/entrypoint.sh

ENTRYPOINT ["/bin/entrypoint.sh"]
