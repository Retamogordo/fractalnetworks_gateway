# Gateway

This is a daemon that controls gateway servers. Gateway servers are servers that
fulfil three major purposes: facilitating connectivity between nodes,
allowing ingress traffic from the internet to reach the nodes, and monitoring
the state of the interfaces and traffic that occurs for accounting purposes.

![Gateway Architecture](gateway.png)

To facilitate network connectivity between nodes, we create wireguard networks
for the nodes to communicate with one another. This is done by creating a Linux
network namespace per group of nodes that want to communicate, and within that
network namespace, offering a wireguard interface. This means that all groups
of nodes that a single gateway hosts is fully isolated.

To allow ingress web traffic to reach the nodes, the gateway servers run
HTTP and HTTPS proxies. HTTP traffic is proxied using a reverse proxy setup,
similar to what is commonly achieved with NGINX. To proxy HTTPS traffic, we
rely on the TLS SNI data: when a client makes a connection, it indicates
to the server which hostname it is connecting to. This is done to allow the
server to use the correct certificate if it is hosting multiple sites.

Instead, we use that connection to then forward the entire encrypted stream
to the respective wireguard network namespace, achieving end-to-end encryption.

# API

The gateway offers a REST API that allows for configuring it. The API is
authenticated with a static token (for now, we will add another authentication
mechanism down the road).

## GET /api/v1/config

Performing a GET request against this endpoint returns the currently active
configuration as a JSON document.

## POST /api/v1/apply

Posting a configuration as a JSON document to this endpoint applies that
configuration. The configuration represents the entire visible state. 

## GET /api/v1/traffic?since=$timestamp

This endpoint returns all traffic data that occured since the supplied timestamp.
Note that traffic information may be deleted after 24 hours.

# State

The state is encoded as a JSON document. An example state is in `test/state.json`,
looking like this:

```json
{
    "12312": {
        "private_key": "2PGDeXYynfKqJH4k0sUgKeRKpL4DUGGLTKnPjKViZFk=",
        "address": ["10.0.0.1/16"],
        "peers": [
            {
                "public_key": "jNBIJrDn1EuvZFmdyTYxobc0lixvWqU3b9mBDKxtWRw=",
                "preshared_key": "4HtDIu03g/UVHHCsKXXRSj7rvA4DidAJ2ryqvCqeWWg=",
                "endpoint": "170.24.12.42:41213",
                "allowed_ips": ["10.0.0.1/32"]
            },
            {
                "public_key": "jNBIJrDn1EuvZFmdyTYxobc0lixvWqU3b9mBDKxtWRw=",
                "preshared_key": "4HtDIu03g/UVHHCsKXXRSj7rvA4DidAJ2ryqvCqeWWg=",
                "endpoint": "170.24.12.42:41213",
                "allowed_ips": ["10.0.0.1/32"]
            },
            {
                "public_key": "jNBIJrDn1EuvZFmdyTYxobc0lixvWqU3b9mBDKxtWRw=",
                "preshared_key": "4HtDIu03g/UVHHCsKXXRSj7rvA4DidAJ2ryqvCqeWWg=",
                "endpoint": "170.24.12.42:41213",
                "allowed_ips": ["10.0.0.1/32"]
            }
        ],
        "proxy": {
            "gitlab.mydomain.com": ["10.0.0.1:8000", "10.0.0.2:5000"],
            "chat.mydomain.com": ["10.0.0.2:7000"]
        }
    }
}
```

The state is essentially a map of ports (which represent the public-facing
WireGuard UDP ports) to network configurations. Every network has a private
key, an address, as well as a bunch of peers. Additionally, networks can have
proxy configurations which forward HTTP and HTTPS traffic from the gateway's
public internet connection to inside the WireGuard networks.

# Implementation

The code is written in Rust, using Rocket as the HTTP library, Tokio for the
asynchronous runtime and SQLite for the database where traffic data is logged.

## Compilation

To compile this code, make sure that you have a nightly version of Rust. If not,
install one with Rustup, by running the installer and then running

    rustup toolchain install nightly

Once that is working, build the code for deployment by running

    cargo +nightly build --release

The resulting static binary will be available in `target/release/gateway` after
a successful build, which can be deployed to any machine. The binary is
self-contained and needs no additional runtime data.

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

*TODO.*
