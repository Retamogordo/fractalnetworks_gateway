#!/bin/bash

# launch NGINX
nginx &

# wait for NGINX to launch
while [ ! -f /run/nginx.pid ]; do
    sleep 0.1
done

# launch gateway (will create database if not exists).
fractal-gateway "$@"
