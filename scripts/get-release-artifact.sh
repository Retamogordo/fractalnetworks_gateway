#!/bin/bash
set -u
mkdir -p target/release
curl --location --output ./target/release/gateway --header "PRIVATE-TOKEN: $GITLAB_TOKEN" "https://gitlab.com/api/v4/projects/30291595/jobs/artifacts/master/raw/target/release/gateway?job=build:$1"
chmod +x ./target/release/gateway


