FROM ubuntu:20.04

ARG BUILD_TYPE=release
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1

# copy entrypoint and binary
COPY target/$BUILD_TYPE/integration /usr/local/bin/integration

ENTRYPOINT ["/usr/local/bin/integration"]

