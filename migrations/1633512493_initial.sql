CREATE TABLE gateway_network(
    network_id INTEGER PRIMARY KEY NOT NULL,
    network_pubkey BLOB NOT NULL UNIQUE
);

CREATE TABLE gateway_device(
    device_id INTEGER PRIMARY KEY NOT NULL,
    device_pubkey BLOB NOT NULL UNIQUE
);

CREATE TABLE gateway_traffic(
    network_id INTEGER NOT NULL REFERENCES gateway_network,
    device_id INTEGER NOT NULL REFERENCES gateway_device,
    time INTEGER NOT NULL,
    traffic_tx INTEGER NOT NULL DEFAULT 0,
    traffic_tx_raw INTEGER NOT NULL,
    traffic_rx INTEGER NOT NULL DEFAULT 0,
    traffic_rx_raw INTEGER NOT NULL,
    PRIMARY KEY (network_id, device_id, time)
);

CREATE INDEX gateway_traffic_time ON gateway_traffic(time);
CREATE INDEX gateway_traffic_device ON gateway_traffic(network_id, device_id);
