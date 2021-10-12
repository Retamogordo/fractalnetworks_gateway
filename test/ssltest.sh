#!/bin/bash

NETWORK_PORT=2000
PEERS=3
TOKEN=
SERVER=localhost:8000

function show_help() {
  printf "USAGE: ssltest.sh [-p PEERS]\n"
  printf "This test simulates a number of peers connecting to the gateway\n"
  printf "and having their SSL traffic forwarded. You must run this as root.\n\n"
  printf "Options\n"
  printf " -p, --peers PEERS\n"
  printf "  Number of peers for each network\n"
}

POSITIONAL=()
while [[ $# -gt 0 ]]; do
  key="$1"
  case $key in
    -t|--token)
      TOKEN="$2"
      shift
      shift
      ;;
    -s|--server)
      SERVER="$2"
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

for n in $(seq $PEERS); do
    printf "Create netns node-$n\n"
    ip netns del "node-$n"
    ip netns add "node-$n"
    mkdir -p "/etc/netns/node-$n/wireguard"
    mkdir -p "/etc/netns/node-$n/opt"
    # create ssl certificate
    openssl req -x509 -newkey rsa:2048 -keyout /etc/netns/node-$n/opt/key.pem -out /etc/netns/node-$n/opt/cert.pem -days 365 -nodes -batch
done

printf "{" > ssltest.json
printf '"%s": {' "$NETWORK_PORT" >> ssltest.json
NETWORK_KEY=$(wg genkey)
printf '"private_key": "%s", ' "$NETWORK_KEY" >> ssltest.json
printf '"address": ["10.0.0.1/16"],' >> ssltest.json
printf '"peers": [' >> ssltest.json
for p in $(seq $PEERS); do
    echo "Configuring node $p"
    printf '{' >> ssltest.json
    PEER_KEY=$(wg genkey)
    printf '"public_key": "%s",' $(echo $PEER_KEY | wg pubkey) >> ssltest.json
    #printf '"preshared_secret": "%s",' $(wg genpsk) >> ssltest.json
    printf '"endpoint": "170.24.12.42:24231",' >> ssltest.json
    printf '"allowed_ips": ["10.0.0.%s/32"]' $(($p + 1)) >> ssltest.json
    if [[ $p == $PEERS ]]; then
        printf '}' >> ssltest.json
    else
        printf '},' >> ssltest.json
    fi
    WGCONF="/etc/netns/node-$p/wireguard/wg0.conf"
    printf "[Interface]\n" > $WGCONF
    printf "Address = 10.0.0.%s/16\n" $(($p + 1)) >> $WGCONF
    printf "PrivateKey = %s\n\n" "$PEER_KEY" >> $WGCONF
    printf "[Peer]\n" >> $WGCONF
    printf "PublicKey = %s\n" $(echo $NETWORK_KEY | wg pubkey) >> $WGCONF
    printf "AllowedIPs = 10.0.0.0/16\n" >> $WGCONF
    printf "Endpoint = 127.0.0.1:%s\n" "$NETWORK_PORT" >> $WGCONF
    printf "PersistentKeepalive = 25\n" >> $WGCONF
    chmod 0500 $WGCONF
done
printf '],' >> ssltest.json
printf '"proxy": {' >> ssltest.json
printf '"https://a.fractal.com": ["10.0.0.2:443"],' $n >> ssltest.json
printf '"https://b.fractal.com": ["10.0.0.3:443"],' $n >> ssltest.json
printf '"https://c.fractal.com": ["10.0.0.4:443"]' $n >> ssltest.json
printf '}' >> ssltest.json
printf '}' >> ssltest.json
printf '}' >> ssltest.json

# apply config
printf "Apply: "
curl -X POST -H "Content-Type: application/json" -H "Token: $TOKEN" -d @ssltest.json "$SERVER/api/v1/config.json"
printf "\n"

for n in $(seq $PEERS); do
    printf "Create wg0 for node-$n\n"
    ip link add "node$n" type wireguard
    ip link set "node$n" netns "node-$n" name "wg0"
    ip -n "node-$n" link set wg0 up
    ip -n "node-$n" link set lo up
    ip -n "node-$n" addr add 10.0.0.$(($n + 1))/16 dev wg0
    ip netns exec "node-$n" wg-quick strip wg0 | ip netns exec "node-$n" wg syncconf wg0 /dev/stdin
    #ip netns exec node-$n ping -c 1 10.0.0.1
    ip netns exec "node-$n" openssl s_server -key /etc/opt/key.pem -cert /etc/opt/cert.pem -accept 443 -www &
done

# wait for servers to exit
wait < <(jobs -p)
