CARGO=cargo
DOCKER=docker
GATEWAY_DATABASE=/tmp/gateway.db
GATEWAY_ADDRESS=127.0.0.1
GATEWAY_PORT=8000
GATEWAY_TOKEN=supersecret

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
	RUST_LOG=info,sqlx=warn RUST_BACKTRACE=1 ROCKET_ADDRESS=$(GATEWAY_ADDRESS) ROCKET_PORT=$(GATEWAY_PORT) $(CARGO) run --release -- --database $(GATEWAY_DATABASE) --secret $(GATEWAY_TOKEN)
