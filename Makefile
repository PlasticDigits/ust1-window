.PHONY: start stop reset wait-healthy test-contracts build-optimized deploy-local verify-oracle-env verify-treasury-wire install-hooks precommit help

start:
	docker compose up -d

stop:
	docker compose down

reset:
	docker compose down -v

wait-healthy:
	@echo "Waiting for LocalTerra RPC..."
	@for i in $$(seq 1 60); do \
		if curl -sf http://localhost:26657/status > /dev/null 2>&1; then \
			echo "LocalTerra is ready"; \
			exit 0; \
		fi; \
		sleep 2; \
	done; \
	echo "ERROR: timeout"; exit 1

test-contracts:
	cargo test -p ust1-common -p ust1-oracle -p ust1-window -p cmm-native-wrap -p ust1-integration-tests

build-optimized:
	chmod +x scripts/optimize.sh && ./scripts/optimize.sh

deploy-local: wait-healthy
	python3 scripts/deploy_local.py

verify-oracle-env:
	bash scripts/verify_oracle_operator_env.sh

verify-treasury-wire:
	bash scripts/verify_treasury_wire_schema.sh

install-hooks:
	@command -v pre-commit >/dev/null || (echo "Install pre-commit: pip install pre-commit (or uv/pipx)"; exit 1)
	pre-commit install

precommit:
	pre-commit run --all-files

help:
	@echo "make start | stop | reset | wait-healthy"
	@echo "make test-contracts | build-optimized | deploy-local | verify-oracle-env"
	@echo "make verify-treasury-wire   (ustr-cmm InstantWithdrawCw20 golden / issue #21)"
	@echo "make install-hooks | precommit   (requires: pip install pre-commit)"
	@echo "Deployment guide: docs/DEPLOYMENT.md"
