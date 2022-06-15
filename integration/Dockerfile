FROM debian:11

ARG BUILD_TYPE=release
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1

# copy entrypoint and binary
COPY target/$BUILD_TYPE/fractal-gateway-integration /usr/local/bin/fractal-gateway-integration

ENTRYPOINT ["/usr/local/bin/fractal-gateway-integration"]

