CARGO=cargo
DOCKER=docker
IMAGE_NAME=registry.gitlab.com/fractalnetworks/gateway
IMAGE_TAG=latest
GATEWAY_DATABASE=/tmp/gateway.db
GATEWAY_ADDRESS=127.0.0.1
GATEWAY_PORT=8000
GATEWAY_TOKEN=supersecret
ARCH=amd64

release:
	$(CARGO) build --release

debug:
	$(CARGO) build

openapi:
	$(DOCKER) run -it --rm -v $(shell pwd):/data openapitools/openapi-generator-cli generate -i /data/api/gateway_0.1.0.yaml -g html2 -o /data/target/openapi
	$(DOCKER) run -it --rm -v $(shell pwd):/data openapitools/openapi-generator-cli generate -i /data/api/gateway_0.1.0.yaml -g openapi -o /data/target/openapi

doc:
	$(CARGO) doc

test:
	$(CARGO) test

run: release
	@touch $(GATEWAY_DATABASE)
	RUST_LOG=info,sqlx=warn RUST_BACKTRACE=1 ROCKET_ADDRESS=$(GATEWAY_ADDRESS) ROCKET_PORT=$(GATEWAY_PORT) sudo $(CARGO) run --release -- --database $(GATEWAY_DATABASE) --secret $(GATEWAY_TOKEN)

deps:
	sudo apt update
	sudo apt install -y wireguard-tools iptables nginx iproute2

docker:
	$(DOCKER) build . -t $(IMAGE_NAME):$(IMAGE_TAG)

docker-push:
	$(DOCKER) push $(IMAGE_NAME):$(IMAGE_TAG)

docker-run:
	-$(DOCKER) network create fractal
	$(DOCKER) run --network fractal --name gateway -it --privileged --rm -p 8000:8000 -p 80:80 -p 443:443 gateway

get-release-artifact:
	./scripts/get-release-artifact.sh $(ARCH)

setup-git:
	git config --global url."ssh://git@gitlab.com".insteadOf "https://gitlab.com"
