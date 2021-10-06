#!/bin/bash

SERVER=localhost:8000
CONTENT=application/json
TOKEN=""
NUMBER=1

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
    -c|--content)
      CONTENT="$2"
      shift # past argument
      shift # past value
      ;;
    -n|--number)
      NUMBER="$2"
      shift # past argument
      shift # past value
      ;;
    *)    # unknown option
      POSITIONAL+=("$1") # save it in an array for later
      shift # past argument
      ;;
  esac
done

for x in $(seq $NUMBER); do
    curl -X POST -H "Content-Type: $CONTENT" -H "Token: $TOKEN" -d @$POSITIONAL "$SERVER/api/v1/config.json"
done
