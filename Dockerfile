FROM ubuntu:20.04

ENV GATEWAY_PORT=8000
ENV GATEWAY_DATABASE=/tmp/gateway.db
ENV GATEWAY_TOKEN=abc

ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=${GATEWAY_PORT}
ENV RUST_LOG=info,sqlx=warn
ENV RUST_BACKTRACE=1

RUN apt update && apt install -y iptables iproute2 wireguard-tools nginx && apt clean
COPY /target/release/gateway /usr/local/bin/gateway
RUN echo "#!/bin/bash\nnginx &\ngateway --database \$GATEWAY_DATABASE --secret \$GATEWAY_TOKEN" > /bin/start.sh
RUN chmod +x /bin/start.sh

ENTRYPOINT ["/bin/start.sh"]
