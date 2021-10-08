CREATE TABLE gateway_traffic(
    network_pubkey BLOB NOT NULL,
    device_pubkey BLOB NOT NULL,
    time INTEGER NOT NULL,
    traffic_tx INTEGER NOT NULL DEFAULT 0,
    traffic_tx_raw INTEGER NOT NULL,
    traffic_rx INTEGER NOT NULL DEFAULT 0,
    traffic_rx_raw INTEGER NOT NULL,
    PRIMARY KEY (network_pubkey, device_pubkey, time)
);

CREATE INDEX gateway_traffic_time ON gateway_traffic(time);
CREATE INDEX gateway_traffic_device ON gateway_traffic(network_pubkey, device_pubkey);
