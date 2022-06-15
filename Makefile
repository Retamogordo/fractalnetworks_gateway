CARGO=cargo
DOCKER=docker
IMAGE_NAME=registry.gitlab.com/fractalnetworks/gateway
IMAGE_TAG=local
ARCH=amd64
BUILD_TYPE=debug

# build in release mode
target/release/fractal-gateway:
	$(CARGO) build --release

# build in debug mode
target/debug/fractal-gateway:
	$(CARGO) build

# build documentation, output in target/doc
doc:
	$(CARGO) doc

# build and run tests
test:
	$(CARGO) test

# install runtime dependencies
deps:
	sudo apt update
	sudo apt install -y wireguard-tools iptables nginx iproute2

# build docker container, set BUILD_TYPE to "debug" or "release"
docker: target/$(BUILD_TYPE)/fractal-gateway
	$(DOCKER) build . --build-arg BUILD_TYPE=$(BUILD_TYPE) -t $(IMAGE_NAME):$(IMAGE_TAG)

# push docker container to gitlab
docker-push:
	$(DOCKER) push $(IMAGE_NAME):$(IMAGE_TAG)

# run docker container
docker-run:
	-$(DOCKER) network create fractal
	$(DOCKER) run --network fractal --name gateway -it --privileged --rm -p 8000:8000 -p 80:80 -p 443:443 gateway

integration: docker
	$(CARGO) build --package fractal-gateway-integration --release
	cd integration && docker-compose --env-file local.env up --build --force-recreate

get-release-artifact:
	./scripts/get-release-artifact.sh $(ARCH)

setup-git:
	git config --global url."ssh://git@gitlab.com".insteadOf "https://gitlab.com"

.PHONY: target/debug/fractal-gateway target/release/fractal-gateway
