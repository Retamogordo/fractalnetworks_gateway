# Gateway

This is a daemon that controls gateway servers. Gateway servers are servers that
fulfil three major purposes: facilitating connectivity between nodes,
allowing ingress traffic from the internet to reach the nodes, and monitoring
the state of the interfaces and traffic that occurs for accounting purposes.

Builds:
- [gateway-amd64][] ([signature][gateway-amd64.sig])
- [gateway-arm64][] ([signature][gateway-arm64.sig])
- [gateway-arm32][] ([signature][gateway-arm32.sig])

Containers:
- [`registry.gitlab.com/fractalnetworks/gateway`][registry]
    - `GATEWAY_PORT`: port to listen on, default 8000.
    - `GATEWAY_TOKEN`: secret authentication token, default `abc`.
    - `GATEWAY_DATABASE`: path to SQLite database, default `/tmp/gateway.db`.

Resources:
- [Source Documentation][rustdoc]
- [API Documentation][openapi]

## Features

Optional features.

- `openapi` ability to generate OpenAPI specification. This adds the `--openapi` command-line option,
  which causes it to print the OpenAPI specification as JSON and exit.

## Building

To build the gateway, use `cargo build`.

    cargo build --release

The binary will then be available in `target/release`.

## Background

To facilitate network connectivity between nodes, we create
[WireGuard][wireguard] networks
for the nodes to communicate with one another. This is done by creating a Linux
network namespace per group of nodes that want to communicate, and within that
network namespace, offering a WireGuard interface. This means that all groups
of nodes that a single gateway hosts is fully isolated.

To allow ingress web traffic to reach the nodes, the gateway servers run
HTTP and HTTPS proxies. HTTP traffic is proxied using a reverse proxy setup,
similar to what is commonly achieved with [NGINX][nginx]. To proxy HTTPS traffic, we
rely on the TLS SNI data: when a client makes a connection, it indicates
to the server which hostname it is connecting to. This is done to allow the
server to use the correct certificate if it is hosting multiple sites.

Instead, we use that connection to then forward the entire encrypted stream
to the respective WireGuard network namespace, achieving end-to-end encryption.

## Dependencies

Install these with APT or similar.

- wireguard-tools
- iptables
- iproute2
- nginx

Additionally, you need to make sure that packet forwarding is enabled in the
kernel. By default, it is disabled. You can enable it with this command:

    sysctl -w net.ipv4.ip_forward=1

This setting will not persist after a reboot, however.

## Running

To run it, simply launch the executable with root privileges on a suitable
Linux machine. To secure it, use the `--token` option to set a secret token
that needs to be present in API calls. To allow it to record traffic stats,
use the `--database` option with a path to a file that will be used to store
traffic data. If no database path is set, traffic data will be stored in RAM
and will not persist after restarts.

Some configuration options can be passed as environment variables:

- `ROCKET_PORT` controls which port the HTTP server listens to, by default 8000.
- `ROCKET_ADDRESS` controls which address the server listens to, by default 127.0.0.1.
- `RUST_LOG` controls how much logging information is output, set to `info` for
  more detail. This can also be used to enable logging only for specific modules
  or functions, for example setting it to `rocket=error,gateway=info` disables
  verbose Rocket output, but still allows all logs from this crate's code.

# License

[AGPL 3.0](LICENSE.md), commercial licensing available upon request.

[sqlite]: https://sqlite.org/
[rust]: https://rust-lang.org/
[wireguard]: https://wireguard.com/
[nginx]: https://nginx.org/
[tokio]: https://tokio.rs/
[rocket]: https://rocket.rs/
[rustup]: https://rustup.rs/

[gateway-amd64]: https://fractalnetworks.gitlab.io/gateway/gateway-amd64
[gateway-arm64]: https://fractalnetworks.gitlab.io/gateway/gateway-arm64
[gateway-arm32]: https://fractalnetworks.gitlab.io/gateway/gateway-arm32

[gateway-amd64.sig]: https://fractalnetworks.gitlab.io/gateway/gateway-amd64.sig
[gateway-arm64.sig]: https://fractalnetworks.gitlab.io/gateway/gateway-arm64.sig
[gateway-arm32.sig]: https://fractalnetworks.gitlab.io/gateway/gateway-arm32.sig

[rustdoc]: https://fractalnetworks.gitlab.io/gateway/doc/gateway
[openapi]: https://fractalnetworks.gitlab.io/gateway/api
[registry]: https://gitlab.com/fractalnetworks/gateway/container_registry

