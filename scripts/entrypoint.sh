#!/bin/bash

# launch NGINX
nginx &

# launch gateway (will create database if not exists).
gateway "$@"
