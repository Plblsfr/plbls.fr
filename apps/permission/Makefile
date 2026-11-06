# Dev et tests. `make help` pour la liste.
.PHONY: help dev-up dev-down dev-run seed test smoke e2e e2e-gateway front-dev image
ENV = set -a; . deploy/dev.env; set +a

help: ## cette aide
	@grep -E '^[a-z-]+:.*##' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-10s %s\n", $$1, $$2}'

dev-up: ## Postgres de dev (Docker, éphémère)
	docker compose -f deploy/docker-compose.dev.yml up -d --wait
dev-down: ## arrête Postgres
	docker compose -f deploy/docker-compose.dev.yml down
dev-run: ## lance le backend sur 127.0.0.1:8090
	cd backend && $(ENV) && cargo run
seed: ## données de départ (applications chat et radar) ; nécessite dev-up + dev-run
	$(ENV) && python3 scripts/seed.py
test: ## tests unitaires Rust
	cd backend && cargo test
smoke: ## test de bout en bout (nécessite dev-up + dev-run)
	$(ENV) && python3 scripts/smoke.py
e2e: ## test du front dans Chromium headless (Docker) : démarre et arrête tout lui-même
	scripts/e2e.sh
e2e-gateway: ## idem avec le vrai auth-gateway + Keycloak de ../security.plbls.fr (connexion admin et demo)
	scripts/e2e-gateway.sh
front-dev: ## Vite sur :3000 dans Docker (pas besoin de Node), proxy signé vers le backend :8090
	docker run --rm -it --network host -u $$(id -u):$$(id -g) -e HOME=/tmp -e npm_config_cache=/tmp/npm -v $(abspath $(CURDIR)/../..):/repo -w /repo node:26-slim \
	  sh -c "npm ci --no-audit --no-fund && npm run dev --workspace apps/permission/frontend -- --host 127.0.0.1"
image: ## image Docker complète (tests + front + backend)
	docker build -f Dockerfile -t permission:local ../..
