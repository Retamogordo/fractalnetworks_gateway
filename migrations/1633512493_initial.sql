CREATE TABLE gateway_network(
    network_id INTEGER PRIMARY KEY NOT NULL,
    network_pubkey BLOB NOT NULL UNIQUE
);

CREATE TABLE gateway_device(
    device_id INTEGER PRIMARY KEY NOT NULL,
    device_pubkey BLOB NOT NULL UNIQUE
);

CREATE TABLE gateway_traffic(
    network_id INTEGER NOT NULL,
    device_id INTEGER NOT NULL,
    time INTEGER NOT NULL,
    traffic_tx INTEGER NOT NULL DEFAULT 0,
    traffix_rx INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (network_id, device_id, time)
);
