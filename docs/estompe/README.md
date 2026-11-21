# Estompe

Messagerie écrite et vocale pour un petit groupe d'amis (messages privés, serveurs, salons texte et vocaux).

- `docs/estompe/` (ce dossier) : [`openapi.yaml`](openapi.yaml) (contrat REST), [`realtime.md`](realtime.md) (évènements WebSocket), [`sablier.md`](sablier.md) (évènements de statistiques émis vers Sablier, import de l'historique), [`AGENTS.md`](AGENTS.md) (consignes des agents), guides
- `api/estompe/` : API en Rust, voir son [README](../../api/estompe/README.md)
- `web/estompe/` : interface Svelte 5 commune à toutes les applications, voir son [README](../../web/estompe/README.md) ; `web/estompe/plateforme/` : plateforme web (PWA) et son image nginx
- `apps/estompe/windows/` : app Windows (Tauri) · `apps/estompe/android/` : app Android (Tauri) · `apps/estompe/crates/core/` : code Rust et TS commun à Windows et Android
- Guides de l'app Windows : [`instant-replay.md`](instant-replay.md) (tampon de jeu en mémoire, clips, galerie), [`editeur-clips.md`](editeur-clips.md) (montage de clips : assembler, rogner, couper, vitesse, recadrage, texte, volumes, export), [`overlay-natif.md`](overlay-natif.md) (overlay sans webview)
- App native en Slint, sans webview (beta) : [`natif.md`](natif.md) (construire, lancer, architecture, captures de rendu), [`natif-parite.md`](natif-parite.md) (ce qui lui manque encore), [`natif-mesures.md`](natif-mesures.md) (mémoire et CPU, point de contrôle du lot 4)
- `deploy/estompe/` : stack Dokploy (images construites par la CI GitHub Actions) et image de job `ci/`
- `scripts/estompe/` : `check.sh` (contrôles avant de livrer), `paquets-cargo.sh`, `bump-version.mjs` et `version.mjs` (versions), `update-policy.mjs`, `natif-manifeste.mjs` et `natif-installeur-check.sh` (livraison de l'app native), `gen-emojis.mjs`
- À la racine du dépôt : `agents/skills/` (skills des agents : `/estompe-grill-me`, `-plan`, `-implement`, `-code-review`, `-pr`, `-release`, lus via `.claude/skills`),
  `changelog.d/` (une entrée de changelog par PR), `CHANGELOG.md` (avancement), `.version` (version de chaque élément : `api`, `web`, `windows`, `android`, `natif`)
