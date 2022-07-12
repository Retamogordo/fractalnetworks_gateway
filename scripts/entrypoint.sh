#!/bin/bash

# clear out any existing namespaces
ip -all netns delete

# launch NGINX
nginx &

# wait for NGINX to launch
while [ ! -f /run/nginx.pid ]; do
    sleep 0.1
done

# launch gateway (will create database if not exists).
fractal-gateway "$@"
