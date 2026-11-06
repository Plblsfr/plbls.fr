# Estompe

Messagerie écrite et vocale pour un petit groupe d'amis (messages privés, serveurs, salons texte et vocaux).

- `docs/init-idea.md` : cahier des charges · `docs/openapi.yaml` : contrat REST · `docs/realtime.md` : évènements WebSocket
- `backend/` : API en Rust, voir son [README](backend/README.md)
- `frontend/` : interface Svelte 5 commune à toutes les applications, voir son [README](frontend/README.md)
- `web/` : image nginx (web, PWA) · `windows/` : app Windows (Tauri) · `android/` : app Android (Tauri) · `crates/core/` : code Rust commun à Windows et Android
- `agents/skills/` : skills des agents (`/estompe-grill-me`, `-plan`, `-implement`, `-code-review`, `-mr`, `-release`), lus via `.claude/skills` · `AGENTS.md` : consignes des agents · `changelog.d/` : une entrée de changelog par PR
- `.version` : version de chaque élément (`api`, `web`, `windows`, `android`)
- `deploy/` : stack Dokploy (images construites par la CI GitHub Actions)
- `CHANGELOG.md` : avancement
