#!/bin/bash
curl -X POST -H "Content-Type: application/json" -d '{"private_key": "EMuuEJRj6IptBdqRirVfrP6qDmr5EBxVblYbOMTNYlM=", "port": 1234, "peers": []}' localhost:8000/api/v1/networks/create
