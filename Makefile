LOCALPATH  ?= `pwd`
DOCKER     ?= docker compose run \
		$(DOCKERFLAGS) \
		--rm \
		$(shell [ -t 0 ] || echo "-T") \
		-e"USER=${USER}" \
		-e"WORKDIR=${WORKDIR}" \
		-e"LOCALPATH=${LOCALPATH}" \
		--volume="/etc/passwd:/etc/passwd" \
		--volume="/etc/shadow:/etc/shadow" \
		--volume="/etc/group:/etc/group" \
		--volume="/var/run/docker.sock:/var/run/docker.sock" \
		--volume="${HOME}:${HOME}" \
		--volume="$(shell pwd):$(WORKDIR)" \
		--workdir="$(WORKDIR)" \
		local
WORKDIR    ?= /workdir

ifneq ("$(wildcard /.dockerenv)","")
    DOCKER=
endif

.PHONY: fmt
fmt: ## format files
	@$(DOCKER) sh -c '\
		cargo fmt ; \
		taplo fmt ; \
	'

.PHONY: lint
lint: ## format files
	@$(DOCKER) sh -c '\
		cargo clippy --all --all-targets --all-features -- -D warnings ; \
		cargo machete ; \
	'

.PHONY: shell
shell: ## start a shell
	@$(DOCKER)


.PHONY: doc
doc: ## generate doc from readme
	@$(DOCKER) sh -c '\
		cargo doc --package pprof_hyper_server --no-deps; \
	'

.DEFAULT_GOAL := help
.PHONY: help
help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'
