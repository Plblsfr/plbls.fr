# CI / CD (GitHub Actions)

La CI est commune au monorepo (`.github/workflows/` à la racine) : un workflow pour les PR, un pour la livraison après fusion,
un pour les apps natives d'Estompe. Runners GitHub (`ubuntu-latest`).

| Workflow | Quand | Rôle |
|---|---|---|
| `ci.yml` | toute PR (un push annule le run précédent de la PR) | contrôles de ce que la PR touche, puis job final `ci` |
| `pr-conventions.yml` | toute PR, et à chaque modification de sa description | nom de branche et description (`scripts/ci-conventions-pr.mjs`) |
| `livraison.yml` | push sur `main`, tags `pli-v*`, `permission-v*`, `security-v*`, `ui-v*`, `estompe-api-v*`, lancement manuel | publie ce que la fusion a changé, sans rejouer les tests |
| `estompe-livraison-app.yml` | tag `estompe-app-v*` ou lancement manuel (case `beta` : appelle `estompe-beta.yml`) | front web, installeur Windows signé, APK, image `ghcr.io/plblsfr/estompe-web` |
| `estompe-beta.yml` | lancement manuel sur la branche d'une PR | check, tests et build du front de la PR, puis image `ghcr.io/plblsfr/estompe-web:beta` (et `beta-pr<n>`, `beta-<sha>`) ; ni `latest`, ni tag, ni release |
| `packages-cleanup.yml` | chaque dimanche, ou à la main | nettoyage des images de `ghcr.io/plblsfr/*` |
| `_changements.yml`, `_image.yml` | appelés par les autres | ce qu'un évènement touche ; build, publication et release d'une image |

Contrôles obligatoires du ruleset `main` (`scripts/configurer-github.sh`) : `ci` et `conventions`. Comme `main` exige une branche
à jour et une fusion classique, le commit fusionné est celui que la PR a testé : `main` ne rejoue pas les tests.

## Ce que touche un évènement (`_changements.yml`)

Les workflows n'ont pas de filtre `paths` (un contrôle obligatoire jamais créé bloquerait la PR). Le job `changements` appelle
`scripts/ci-changements.sh --nom <groupe> <chemins>` pour chaque groupe (préfixe `!` pour exclure, syntaxe glob de git : `**/*.md`
et non `**.md`) et expose une sortie `true`/`false` par groupe. En PR comme sur un push de `main`, il compare le commit de fusion à son
premier parent ; sur un tag ou un lancement manuel, tout est à `true`. En PR, modifier `ci.yml`, `_changements.yml` ou
`scripts/ci-changements.sh` met tout à `true` (et `_image.yml`, les groupes d'image). Les fichiers `*.md` ne déclenchent que
le groupe `lint` (job `front` sans espace npm : liens Markdown internes et `just lint`) et le contrôle de changelog d'Estompe : une PR de
documentation seule ne lance que `conventions`, ce `front` léger (et `changelog` si elle touche un dossier d'Estompe).

| Groupe | Chemins | Jobs |
|---|---|---|
| `rust` | `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml`, tout `Dockerfile`, `api/**` (membres du workspace) | `rust-lint` |
| `estompe_api` | `api/estompe/**`, `deploy/estompe/.env.example`, `docs/estompe/openapi.yaml`, `scripts/estompe/paquets-cargo.sh`, fichiers Cargo racine, `.dockerignore` | `rust-test` (Estompe), `estompe-api-image` ; sur `main` : image `:edge` de l'API |
| `pli_server` | `api/pli/**`, fichiers Cargo racine | `rust-test` (pli) |
| `estompe_tauri` | `apps/estompe/**` (`windows`, `android`, `crates`) | `estompe-tauri` |
| `estompe` | `{api,web,apps,docs,deploy,scripts}/estompe/**`, `.version`, `CHANGELOG.md`, `changelog.d/**`, `agents/**` | `changelog` |
| `lint` | JS, TS, Svelte, CSS, JSON, shell, Python, Markdown, configs des linters, `scripts/**` | `front` (`just lint`, dont les liens Markdown internes) |
| `npm`, `front_*` | fichiers npm racine, `scripts/*` ; chaque projet npm (`web/estompe` compte aussi le TS des apps natives et `scripts/estompe/*.mjs`) | `front` : check, test, build des projets touchés (tous si les fichiers npm racine changent) |
| `image_*` | dossier du projet, fichiers npm et Cargo racine selon l'image | `image-<projet>` (PR : étape de test du Dockerfile et build à blanc ; `main` : `:edge`) |
| `estompe_version` | `api/estompe/**`, `web/estompe/**`, `apps/estompe/**` hors `*.md` | `livraison.yml` : `estompe-version` |
| `estompe_ci_image` | `deploy/estompe/ci/**` | `livraison.yml` : `estompe-ci-image` |

## `ci.yml` (PR)

- `changelog` : exige un nouveau `changelog.d/*.md` ; exempté pour `ci/`, `docs/`, `release/` et Dependabot.
- `front` : job générique des projets npm (un seul `npm ci` du `package-lock.json` racine) : version de Node, copies de `@plbls/ui`,
  tests des scripts communs, liens Markdown internes (`scripts/check-links.mjs`), `just lint` si le groupe `lint` est touché, `npm audit` d'Estompe, puis `check`, `test` et `build` de chaque
  espace npm touché (sortie `espaces` de `changements`). Le futur front unique s'y branche en devenant un espace npm de plus (ou le seul).
- `rust-lint` : une seule fois pour tout le workspace : toolchain de `rust-toolchain.toml`, version de Rust des `FROM rust:` des Dockerfiles,
  `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, règle « le cœur d'Estompe ne dépend d'aucun bloc »,
  `cargo deny check`.
- `rust-test` : tests des crates d'Estompe (PostgreSQL 17, port 5432) et du serveur pli (PostgreSQL 16, port 5433), chacun selon son groupe.
  Les services tournent à côté du runner : joignables par `localhost` et leur port publié. Une base par test sur un seul serveur.
- `estompe-tauri` : tests des apps Tauri (GTK/WebKit), `cargo check` de l'app Android, Bobyn, ffmpeg épinglé.
- `estompe-api-image` : construit l'image de l'API sans la publier (cache `:buildcache` du registre en lecture), la démarre à côté d'un
  PostgreSQL et attend `GET /readyz` (60 s au plus). Conteneurs et tag portent l'identifiant du run.
- `image-pli`, `image-permission`, `image-security`, `image-ui` : `_image.yml` sans publication (étape `test` du Dockerfile de security
  et permission, `build` de ui).
- `ci` : toujours créé ; échoue si un job a échoué ou a été annulé (un job ignoré compte comme réussi).
- Transition : le job `anciens-controles` recopie le résultat de `ci` sous les anciens noms (`npm`, `estompe-backend-rs`, `estompe-frontend`,
  `pli-ci`, `permission-ci`, `security-ci`, `ui-ci`) tant que le ruleset les exige ; il disparaît une fois `scripts/configurer-github.sh` relancé.

## `livraison.yml` (main et tags)

- `image-<projet>` (`_image.yml`) : après une fusion qui touche le projet, image `:edge` + `:<sha>` ; sur le tag `<composant>-vX.Y.Z`, image
  `:X.Y.Z` + `:latest`, release GitHub et webhook Dokploy (environnement `livraison`). L'étape de test du Dockerfile n'est pas rejouée.
- `ui-dist` : tag `ui-v*` : archives de `packages/ui/dist` jointes à la release.
- `estompe-api-image` (+ `estompe-api-release` sur tag) : image `ghcr.io/plblsfr/estompe` (`edge` sur `main`, `X.Y.Z`, `latest`, `sha-…` sur le
  tag `estompe-api-vX.Y.Z`), cache d'image `:buildcache` alimenté ici.
- `estompe-version` : calcule `.version` (nom de branche lu dans le message du commit de fusion, voir `docs/estompe/AGENTS.md`), le pousse sur
  `ci/version-<sha>` et ouvre une PR « chore(version) » (secret `CI_PAT` : avec `GITHUB_TOKEN`, la PR ne déclencherait aucune CI). Fusionner cette PR
  avant de poser un tag `estompe-api-v*` / `estompe-app-v*`. Relancé, il ne fait rien de plus : branche existante ou `.version` inchangé.
  Ignoré si le message du commit contient `[skip ci]`.
- `estompe-ci-image` : image de job `ghcr.io/plblsfr/estompe-ci` ; incrémenter `:4` dans `livraison.yml`, `deploy/estompe/ci/Dockerfile` et `estompe-livraison-app.yml`.
- `cache-rust`, `cache-npm` : écrivent sur `main` les caches lus par les PR (voir plus bas).

Un tag ne déclenche que son composant. Lancé à la main sur `main`, tout est republié (images `:edge`, version, caches).

Connexion à ghcr.io (publication des images, image de job `estompe-ci`) : secret `PACKAGES_PAT` (PAT classique du propriétaire des paquets,
portées `write:packages` et `delete:packages`, partagé avec `packages-cleanup.yml`) s'il existe, sinon `GITHUB_TOKEN`. Le `GITHUB_TOKEN` n'écrit que dans les paquets dont l'accès Actions inclut `plbls.fr`
en écriture : les paquets créés par les anciens dépôts le refusent (`denied: permission_denied: read_package`).

## Caches

- Une PR ne lit que les caches de sa branche et de `main` : les caches partagés sont donc écrits sur `main` par `livraison.yml`, et seulement
  quand leur clé n'existe pas encore (recherche `lookup-only`, quelques secondes sinon).
- Cargo (workspace) : deux caches pour tout le workspace, `rust-lint-v2-` (clippy) et `rust-test-v1-` (tests), clé sur `Cargo.lock` et
  `rust-toolchain.toml`, repli sur le préfixe. `actions/cache/restore` puis `save` explicites ; une PR n'écrit que si la clé exacte manque.
  `CARGO_INCREMENTAL=0` et sans infos de débogage pour réduire la taille. `cargo-deny` a son propre cache par version.
- Cargo (apps Tauri) : `tauri-test-v1-`, clé sur leurs `Cargo.lock`, écrit par les PR seulement.
- npm : `actions/setup-node` avec `node-version-file: .nvmrc` et `cache: npm` (un seul `package-lock.json`).
- Docker : `type=gha` par image pour `_image.yml`, `:buildcache` dans le registre pour l'API Estompe. Le `Dockerfile` Rust compile les
  dépendances dans leur couche (`cargo-chef`).
- `Swatinem/rust-cache` non retenu : action tierce de plus à autoriser (`patterns_allowed` de `scripts/configurer-github.sh`) pour un gain
  faible ; `actions/cache` suffit avec des clés communes.

## Ajouter un projet

Un groupe de chemins dans `_changements.yml` (et sa sortie), un job conditionné dans `ci.yml` ajouté aux `needs` de `ci` (ou, pour un projet
npm, une ligne dans la liste `espaces`), et s'il publie une image, un job dans `livraison.yml`. Le ruleset ne change pas.

## Version beta d'une PR (`estompe-beta.yml`)

Pour essayer le front d'une PR branché sur la prod sans l'imposer à tout le monde. Actions → `estompe-beta` → « Run workflow »
sur la branche de la PR (menu « Use workflow from » ; le workflow refuse `main` et les tags), avec le numéro de la PR pour nommer le tag `beta-pr<n>`.
Le workflow ne récupère jamais le code d'une autre référence que celle où il tourne : le code de la PR ne s'exécute pas avec les droits de `main`. Même chose depuis `estompe-livraison-app` avec la case `beta` cochée :
il appelle `estompe-beta.yml` et ne lance rien d'autre (utile tant qu'`estompe-beta.yml` n'est pas sur `main`, GitHub ne proposant le
lancement manuel que des workflows présents sur la branche par défaut). Le workflow lance `check`, les tests et le build du front de la PR,
puis publie `ghcr.io/plblsfr/estompe-web:beta` (dernière beta), `beta-pr<n>` et `beta-<sha>`. Il ne touche ni `latest`, ni les tags,
ni les releases, ni les apps Windows et Android. `packages-cleanup.yml` garde `beta` et supprime les autres tags beta chaque semaine.

Côté infra (Dokploy) : un service à côté de `estompe-web`, avec l'image `ghcr.io/plblsfr/estompe-web:beta` et les mêmes variables
(`API_URL`, `GATEWAY_URL`, `VOICE_URL`), sur un sous-domaine de `plbls.fr` (le cookie de session est posé sur `.plbls.fr`), par
exemple `beta.estompe.plbls.fr`. Ajouter cette origine à `ALLOWED_ORIGINS` de l'API, et à la liste des adresses de retour du gateway
si elle est restreinte. Redémarrer le service après chaque publication pour qu'il prenne la nouvelle image. La beta travaille sur les
données de prod : seul le front change, l'API reste celle de la prod, donc une PR qui modifie l'API ne se teste pas ainsi.
