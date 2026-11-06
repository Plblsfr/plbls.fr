# Environnement de dev et tests. `make help` pour la liste.
.PHONY: help dev-up dev-start dev-stop dev-down dev-reset dev-logs dev-status test bruno ws refresh security coverage test-all

help: ## cette aide
	@grep -E '^[a-z-]+:.*##' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-12s %s\n", $$1, $$2}'

dev-up: ## Redis + Keycloak + service d'écho (Docker, attend qu'ils soient prêts)
	scripts/dev.sh up
dev-start: ## build et lance le gateway en arrière-plan
	scripts/dev.sh start
dev-stop: ## arrête le gateway
	scripts/dev.sh stop
dev-down: ## arrête tout
	scripts/dev.sh down
dev-reset: ## arrête tout et supprime les volumes
	scripts/dev.sh reset
dev-logs: ## suit les logs du gateway
	scripts/dev.sh logs
dev-status: ## état des conteneurs et du gateway
	scripts/dev.sh status

test: ## tests unitaires Rust
	cargo test
bruno: ## suite Bruno (tous les endpoints), via Docker : nécessite dev-up + dev-start
	scripts/bruno.sh
ws: ## test WebSocket (proxy) : nécessite dev-up + dev-start
	python3 scripts/ws_test.py
refresh: ## test du refresh de session et de la concurrence : nécessite dev-up + dev-start
	python3 scripts/refresh_test.py
security: ## régressions de sécurité (traversée de chemin, login CSRF, logout) : nécessite dev-up + dev-start
	python3 scripts/security_test.py
coverage: ## vérifie que chaque route du code est couverte par la suite Bruno
	python3 scripts/check_coverage.py

test-all: test coverage dev-up dev-start bruno ws refresh security ## tout, de bout en bout
