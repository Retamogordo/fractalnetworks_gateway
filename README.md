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

# License
