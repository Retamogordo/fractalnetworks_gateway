#!/bin/bash

AMOUNT=10
PORT_START=2000
PEERS=3

function show_help() {
  printf "USAGE: generate.sh [-a AMOUNT] [-p PEERS]\n"
  printf "Generates random state for gateway with specific number of\n"
  printf "networks and peers.\n\n"
  printf "Options\n"
  printf " -a, --amount AMOUNT\n"
  printf "  Number of networks to create\n"
  printf " -p, --peers PEERS\n"
  printf "  Number of peers for each network\n"
}

POSITIONAL=()
while [[ $# -gt 0 ]]; do
  key="$1"
  case $key in
    -a|--amount)
      AMOUNT="$2"
      shift
      shift
      ;;
    -p|--peers)
      PEERS="$2"
      shift
      shift
      ;;
    -h|--help)
      show_help
      exit
      ;;
    *)
      show_help
      exit -1
      ;;
  esac
done

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
    printf '"ssh://git.domain%s.com": ["10.0.0.1:8000", "10.0.0.2:7000"],' $n
    printf '"https://gitlab.domain%s.com": ["10.0.0.2:443", "10.0.0.5:443"],' $n
    printf '"https://chat.domain%s.com": ["10.0.0.3:6000"]' $n
    printf '}'
    if [[ $n == $AMOUNT ]]; then
        printf '}'
    else
        printf '},'
    fi
done

printf '}'
