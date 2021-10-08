#!/bin/bash

SERVER=localhost:8000
TOKEN=""
START_TIME=0

POSITIONAL=()
while [[ $# -gt 0 ]]; do
  key="$1"
  case $key in
    -t|--token)
      TOKEN="$2"
      shift # past argument
      shift # past value
      ;;
    -s|--server)
      SERVER="$2"
      shift # past argument
      shift # past value
      ;;
    -s|--start)
      START_TIME="$2"
      shift # past argument
      shift # past value
      ;;
    *)    # unknown option
      POSITIONAL+=("$1") # save it in an array for later
      shift # past argument
      ;;
  esac
done

curl -s -H "Token: $TOKEN" "$SERVER/api/v1/traffic.json?start=$START_TIME"
