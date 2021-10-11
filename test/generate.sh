#!/bin/bash

AMOUNT=100
PORT_START=2000
PEERS=10

printf "{"
for n in $(seq $AMOUNT); do
    PORT=$(($PORT_START + $n))
    printf '"%s": {' "$PORT"
    printf '"private_key": "%s", ' $(wg genkey)
    printf '"address": ["10.0.0.1/16"],'
    printf '"peers": ['
    for p in $(seq $PEERS); do
        printf '{'
        printf '"public_key": "%s",' $(wg genkey | wg pubkey)
        printf '"preshared_secret": "%s",' $(wg genpsk)
        printf '"endpoint": "170.24.12.42:24231",'
        printf '"allowed_ips": ["10.0.0.%s/32"]' $(($p + 1))
        if [[ $p == $PEERS ]]; then
            printf '}'
        else
            printf '},'
        fi
    done
    printf '],'
    printf '"proxy": {'
    printf '"git.domain%s.com": ["10.0.0.1:8000", "10.0.0.2:7000"],' $n
    printf '"chat.domain%s.com": ["10.0.0.3:6000"]' $n
    printf '}'
    if [[ $n == $AMOUNT ]]; then
        printf '}'
    else
        printf '},'
    fi
done

printf '}'
exit

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
    },
