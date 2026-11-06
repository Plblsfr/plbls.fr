# AGENTS.md — consignes pour les agents

Estompe : messagerie écrite et vocale (messages privés, serveurs, salons texte et vocaux). Backend Rust, frontend Svelte 5
(web, PWA, apps Windows et Android en Tauri), vocal LiveKit. Lire d'abord `README.md`, `backend/README.md` (dont « Ajouter un bloc ») et `frontend/README.md`.

Langue : le code, les messages de commit et les PR sont **en français**. Aucun commentaire dans le code.

Ce fichier est la référence : les skills (voir « Skills ») le citent au lieu de le recopier. Un changement de règle se fait ici
puis dans les skills concernés, dans la même PR.

## Règle n°1 : chaque demande repart de `main`

1. Une demande = une branche = un worktree = une PR, créés depuis `origin/main` à jour :
   `git fetch origin && git worktree add -b <type>/<slug-court> .worktree/<slug-court> origin/main`.
2. Ne jamais empiler une demande sur la branche d'une autre, même en cours de relecture : les PR restent indépendantes.
3. Ne jamais commiter sur `main`. Ne pas réécrire l'historique d'une branche déjà poussée sans demande explicite.
4. `<slug-court>` : mots-clés en minuscules séparés par des tirets, sans numéro de ticket (ex. `sondages-multiples`).

## Dossiers locaux (jamais commités)

| Dossier | Contenu |
|---|---|
| `.worktree/<slug-court>/` | un worktree par branche de travail |
| `.ai/EST-<n>/` | fichiers de travail d'un ticket : `brief.md`, `plan.md` (revues comprises), `verify.md` |

- Les deux sont à la racine du dépôt principal (parent de `git rev-parse --path-format=absolute --git-common-dir`), jamais dans un worktree.
- Ils sont exclus par `.git/info/exclude` (lignes `.ai/` et `.worktree/`), pas par `.gitignore`.
- `.ai/` n'est pas partagé : ce qui doit être connu de tous (décisions, critères, lien de la PR) va aussi en commentaire du ticket Cairn.

## Skills et cycle d'un ticket

Guide pas à pas : `docs/ticket-claude-code.md`. Les skills sont versionnés dans `agents/skills/<nom>/SKILL.md` ; `.claude/skills` y pointe pour Claude Code. Un autre agent lit ces fichiers comme des procédures.

| Skill | Produit | Effet |
|---|---|---|
| `/estompe-grill-me EST-<n>` | `.ai/EST-<n>/brief.md` | questions jusqu'à lever toute ambiguïté ; ticket « en cours » |
| `/estompe-plan EST-<n>` | `.ai/EST-<n>/plan.md` | branche, étapes, tests, couverture de chaque critère |
| `/estompe-implement EST-<n>` | worktree, commits locaux, `.ai/EST-<n>/verify.md` | suit le plan (ou les corrections), lance `scripts/check.sh`, prouve chaque critère |
| `/estompe-code-review EST-<n>` | section « Revue N » de `plan.md` | revue 1 : trois relecteurs filtrés ; revue 2 : corrections seules ; rapport seul |
| `/estompe-mr EST-<n>` | `changelog.d/<slug-court>.md`, push, PR | commentaire Cairn, puis suppression du worktree |
| `/estompe-release` | release, tags, release GitHub | tickets livrés passés « fini » si tous leurs critères sont vérifiés |

Règles du cycle :
- Un ticket se désigne toujours par sa clé complète `EST-<n>` : Cairn refuse le numéro seul, et `#<n>` renvoie vers une issue GitHub.
- Les critères de vérification du brief sont numérotés `C1`, `C2`… et suivis jusqu'à la release.
- Boucle courte : `implement` → `code-review` (revue 1) → si besoin `implement` (corrections) → `code-review` (revue 2, corrections seules) → `mr`.
  **Deux revues au plus** : seuls les bloquants et importants prouvés bloquent ; les mineurs vont dans la PR, tout nouveau défaut hors diff devient un ticket.
  `/estompe-mr` relance `scripts/check.sh` lui-même et accepte une revue antérieure si les commits suivants ne sont que ses corrections.
- Après la création de la PR, `/estompe-mr` supprime le worktree (`git worktree remove`) ; la branche reste. Pour corriger la PR, `/estompe-implement` le recrée depuis la branche.
- Un ticket reste « en cours » jusqu'à la release. Il ne passe « fini » que si tous ses critères sont vérifiés ; sinon il reste « en cours »
  avec la liste de ce qui reste à vérifier. Un critère qui dépend d'un appareil réel reste « non vérifié » tant que la personne qui l'a n'a pas confirmé.
- Un bug se reproduit (test qui échoue) avant d'être corrigé. Sans reproduction possible, le dire sans affirmer de cause.
- Rester agnostique du système : aucune hypothèse sur un OS ou un appareil particulier.

## Nom de la branche = numéro de version

Le préfixe de la branche pilote la version de chaque élément (`api`, `web`, `windows`, `android`), stockée dans `.version` et mise à jour
par la CI à la fusion (`frontend/scripts/bump-version.mjs`, workflow `version.yml`) :

| Branche | Effet à la fusion dans `main` |
|---|---|
| `feat/…` | version **minor** +1 des éléments concernés (0.2.3 → 0.3.0) |
| `fix/…`, `chore/…`, `docs/…`, `ci/…`, tout autre | version **patch** +1 des éléments concernés (0.2.3 → 0.2.4) |
| `release/…` | **aucun effet** : réservé à la PR qui prépare une release |
| majeure | **à la main** : modifier la valeur dans `.version` |

Détails :
- Éléments concernés = dossiers modifiés par la fusion : `backend/` → api ; `web/` → web ; `windows/` → windows ; `android/` → android ;
  `frontend/` → web, windows et android ; `crates/` → windows et android. Une fusion qui ne touche que `docs/`, la CI… ne change rien.
- La CI calcule `.version` et ouvre une PR `ci/version-<sha>` (« chore(version) ») : `main` est protégée, la CI n'y pousse pas. Fusionner cette PR (« merge »)
  avant de poser un tag de livraison. Ne pas modifier `.version` dans une autre PR, sauf pour une version majeure.
  Au build, `bump-version.mjs --apply` recopie les versions de `windows` et `android` dans `tauri.conf.json`, `Cargo.toml` et `Cargo.lock`
  (l'espace de travail seulement) ; l'app ne propose la mise à jour que si la version publiée a augmenté.
- Le nom de la branche est lu dans le message du commit de fusion de GitHub (« … from feat/x into main »). **Fusionner en
  « merge » classique** : en squash ou rebase, le nom est perdu et la version monte en patch.
- Choisir le préfixe selon l'effet voulu : une fonctionnalité visible par les utilisateurs → `feat/`.

## Release et livraison

Branche `release/…`, `changelog.json`, forçage de mise à jour, tags `estompe-api-v*` et `estompe-app-v*` : voir `docs/release.md` (lu par `/estompe-release`).
Ne jamais écraser ni supprimer un tag existant ; pousser un tag seul (`push origin <tag>`), jamais `--tags`.

## Git et GitHub

- Dépôt : monorepo `https://github.com/Plblsfr/plbls.fr`, Estompe vit dans `apps/estompe/`. Les tags sont préfixés : `estompe-api-v*`, `estompe-app-v*`.
- **Seul `gh` parle à GitHub** (PR, tickets de dépôt, releases…). Interdits : `curl`/`wget` vers `api.github.com`, appel à l'API à la main,
  bibliothèque HTTP. Les règles `deny` de `.claude/settings.json` le font respecter.
- `gh` doit être connecté : `gh auth status` doit répondre. Sinon, s'arrêter et le dire à l'utilisateur.
- Le transport git (`fetch`, `pull`, `push`, tags) passe par `git` avec l'authentification de `gh`. Ne jamais forcer un push.
- PR : `gh pr create --base main --head <branche> --title "<titre>" --body "<corps>"`.
  Corps : contenu, critères et vérifications, et ce qui n'a pas été vérifié.
- Pousser et ouvrir une PR seulement via `/estompe-mr` ou `/estompe-release`, ou sur demande explicite de l'utilisateur.
- Messages de commit :
  - travail sur un ticket : `[EST-<n>] <titre du ticket>`, puis `[EST-<n>] Corrections de revue`, `[EST-<n>] Changelog` ;
  - release : `chore(release): api-vA.B.C app-vX.Y.Z` ;
  - hors ticket : `type(portée): résumé` (`feat`, `fix`, `chore`, `ci`, `docs`).

## Vérifier avant de livrer

Depuis la racine du worktree, sans `cd … &&` :

```sh
scripts/check.sh
```

Le script lance, dans l'ordre : `TEST_DATABASE_URL` défini, `cargo fmt --check`, `clippy -D warnings` (tout le workspace racine), `cargo test` des crates d'Estompe, `cargo deny check`, `npm --prefix frontend run check`, `npm --prefix frontend test`. Il est la seule liste de contrôles : skills et CI s'y réfèrent.

Pendant le développement, du plus rapide au plus complet : `cargo check $(scripts/paquets-cargo.sh) --all-targets`, `cargo build $(scripts/paquets-cargo.sh)`, `cargo test $(scripts/paquets-cargo.sh) --no-run`, `cargo test $(scripts/paquets-cargo.sh)`. En local, `blaze` (indexable-inc) remplace `cargo` pour `build`, `check`, `run` et `test` (recompilation incrémentale en mémoire, voir `backend/README.md`) ; `scripts/check.sh` et la CI restent sur `cargo` et la toolchain épinglée, qui font foi. Aucun linker à configurer : mesuré, mold n'apporte rien sur le linker par défaut (`backend/README.md`, « Développement »).

- Rust : les crates du backend sont membres du workspace Cargo racine du monorepo (`../../Cargo.toml`, un seul `Cargo.lock`, `deny.toml` et
  lints clippy communs avec `apps/security` et `apps/permission/backend`). La toolchain (1.99.0) est épinglée par `../../rust-toolchain.toml`,
  lu depuis tout le dépôt sauf `windows/`, `android/` et `crates/` (apps Tauri, hors workspace, sur la stable). `scripts/paquets-cargo.sh` donne les `-p` des crates d'Estompe.
  `cargo-deny` s'installe avec `cargo install cargo-deny --locked`.
- Plus de Docker en local. Les tests Rust utilisent le PostgreSQL de `TEST_DATABASE_URL` (compte superutilisateur, une base créée puis
  supprimée par test) : ils valident les migrations et les requêtes. Sans cette variable, ils essaient un conteneur (testcontainers, en CI)
  et sont sinon ignorés (« ignoré : Docker indisponible », test réussi) : vérifier la sortie.
  Toute base créée à côté de `testutil::postgres()` (modèle, copies) doit être rattachée par `BasePostgres::supprimer_aussi`, sinon elle reste sur le serveur.
- Front : `npm --prefix frontend run …` s'exécute dans `frontend/` et lit sa config ; `npx --prefix frontend …` lancé depuis la racine ne la lit pas.
- Dans un nouveau worktree, installer d'abord les dépendances front : `npm --prefix ../.. ci` (workspace npm à la racine du dépôt, un seul `package-lock.json`, Node du `.nvmrc` racine).
- Chaque PR ajoute un fichier `changelog.d/<slug-court>.md` (voir `changelog.d/README.md`) qui cite `EST-<n>`, sauf branches `ci/`, `docs/`,
  `release/` : la CI échoue sinon. Elle lance les tests de tout (backend, front, Rust).
- Dire clairement ce qui n'a **pas** été vérifié (rendu visuel, build Tauri/Windows, vrai partage d'écran…) dans la réponse et dans la PR.

## Tests

- Un changement de logique non triviale s'accompagne d'un test.
- Les nouveaux tests vont dans le fichier de test existant du code testé (Rust : module `#[cfg(test)]` du fichier testé ;
  front : `src/lib/<module>.test.ts`). Ne jamais créer de nouveau fichier ou de nouvelle classe de test quand un existe déjà pour ce code.
- Un test attend une condition, jamais un délai fixe.

## Architecture

- `backend/` (Rust, décisions dans `docs/migration-rust.md`) : workspace Cargo avec `estompe-api` (binaire), `core`, `block` et
  `blocks/<nom>` (un crate par bloc, mêmes noms que les blocs frontend). `core` ne dépend d'aucun bloc ; un bloc dépend de `core`, de `block`
  et d'un autre bloc seulement s'il le déclare dans `Needs` ; `estompe-api` assemble tout ; aucune dépendance circulaire.
- **Blocs** : un cœur et des fonctionnalités optionnelles. Backend Rust : un crate `backend/blocks/<nom>` par bloc, déclaré dans `block_list()`
  (`backend/estompe-api/src/main.rs`) avec son nom et ses dépendances (`needs`) ; `BLOCK_<NOM>=false` le désactive (routes absentes) ;
  le serveur annonce les blocs actifs dans `blocks` de `GET /me`. Frontend : `frontend/src/blocks/<nom>/` avec les mêmes noms (`registry.ts`),
  `app.on('<nom>')` masque un bloc inactif. Une nouvelle fonctionnalité optionnelle = un nouveau bloc des deux côtés, documenté dans
  `deploy/.env.example` et `deploy/docker-compose.yml`. **Procédure complète : `backend/README.md`, section « Ajouter un bloc ».**
- `frontend/` : code commun à toutes les applications. `src/components` et `src/views` : cœur (Svelte 5, runes) ; `src/lib` : état
  (`store.svelte.ts`, `voice.svelte.ts`, `prefs.svelte.ts`, `api.ts`) et interface `Platform` (`platform.ts`). UI : `plbls-ui` embarquée
  dans `src/vendor/plbls-ui`.
- Une plateforme = un dossier : `web/` (image nginx, `docker/`), `windows/` et `android/` (coques Tauri : `src/platform.ts`,
  `src-tauri/`, `scripts/`). `crates/core/` : code Rust et TS commun aux deux apps Tauri. Le frontend ne teste jamais la plateforme
  (`isDesktop`, agent) : il appelle `platform` ; un comportement propre à une plateforme s'ajoute dans son dossier (voir `frontend/README.md`).
- `.version` : version de chaque élément (api, web, windows, android).
- `docs/` : `openapi.yaml` (REST) et `realtime.md` (évènements WebSocket). **Toute nouvelle route ou nouvel évènement est documenté ici.**
- `deploy/` : stack Dokploy. Toute nouvelle variable d'environnement va dans `deploy/.env.example` et `deploy/docker-compose.yml`.
- `.github/workflows/estompe-*.yml` (déclencheurs actuels et à réactiver : `docs/ci.md`) : `backend-rs.yml` (API Rust : fmt, clippy, cargo-deny, tests avec PostgreSQL,
  build et démarrage de l'image), `frontend.yml` (front : audit, types, tests,
  build ; tests Rust des apps), `changelog.yml` (PR : entrée `changelog.d/` exigée), `version.yml` (fusion dans `main` : calcul de `.version`),
  `ci-image.yml` (image de job de `deploy/ci/`), `image.yml` (tag `api-v*` : image Rust de l'API), `livraison-app.yml` (tag `app-v*` : web, installeur Windows, APK).
  Une fusion dans `main` ne construit aucune livraison : seuls les tags livrent.

## Conventions

- **Migrations** : ajouter `NNN_nom.sql` (numéro suivant) dans `backend/core/migrations/`. Ne jamais modifier une migration déjà fusionnée dans `main` : en ajouter une nouvelle.
- **Temps réel** : une modification d'état visible par d'autres passe par un évènement WebSocket publié par l'API
  (`publishMessage`) et géré dans `store.svelte.ts` (`onEvent`) ; prévoir le test dans `store.test.ts`.
- **Design** : l'interface suit `plbls-ui` ; pas de refonte visuelle avant la v2 de plbls-ui (bêta ouverte).
- **Sécurité** : valider toute entrée côté serveur (longueurs, formats, droits : auteur du message, membre du serveur) ;
  le client n'est jamais une source de confiance (ex. plafonds du partage d'écran annoncés par le serveur).
- **Desktop (Tauri)** : le webview n'envoie pas les mêmes en-têtes qu'un navigateur ; les appels à Keycloak et au gateway
  passent par Rust (`native_fetch`). La CSP est dans `windows/src-tauri/tauri.conf.json` (et `android/`).
- Code minimal : réutiliser l'existant (helpers, composants, patterns) avant d'ajouter ; pas de dépendance pour quelques lignes.

## Pièges connus

Quand un ticket révèle un piège, la PR l'ajoute ici.

- **Config cargo** : cargo lit `.cargo/config.toml` depuis le dossier courant et ses parents, jamais depuis `--manifest-path` : `scripts/check.sh` lance cargo depuis la racine du monorepo, un fichier dans `backend/` y est ignoré.
- **Workspace racine** : une dépendance ajoutée ou montée dans `Cargo.toml` racine touche aussi security et permission ; leurs CI se déclenchent sur ce fichier, `Cargo.lock`, `rust-toolchain.toml` et `deny.toml`.
- **Champs tactiles** : iOS zoome sur un champ sous 16 px et ne dézoome pas ; la règle `@media (pointer: coarse)` d’`app.css` les force à 16 px, ne jamais poser `maximum-scale` ni `user-scalable`.
- **Lecteur YouTube** : toujours l'iframe officielle avec `referrerpolicy="strict-origin-when-cross-origin"` (sans `Referer`, YouTube
  répond par l'erreur 153). L'origine de la webview Tauri est `https://tauri.localhost` : tester la lecture dans l'app, pas seulement dans un navigateur.
- **Lecture automatique** : le navigateur ou la webview peut la refuser (`onAutoplayBlocked`) ; prévoir le bouton « Activer le son », ne jamais supposer que `play()` aboutit.
- **Empilement** : un menu ouvert dans un modal doit s'afficher au-dessus de lui sans être coupé par un `overflow` masqué ; réutiliser
  `ContextMenu` / `Modal` plutôt qu'un menu local, et ne pas régler un souci d'empilement par un `z-index` arbitraire.
- **Composants partagés obligatoires** : `Modal`, `ConfirmModal`, `ContextMenu`, `Icon`, `Avatar`… avant d'en écrire un autre ; jamais `window.confirm`, `alert` ni `prompt`.
- **Films (Jellyfin)** : les applications n'appellent que l'API ; ne jamais renvoyer une réponse Jellyfin telle quelle ni un identifiant ou chemin Jellyfin (identifiants publics `films_items`) ; tout flux passe par une session de flux signée (`flux.rs`) ; un nouveau type de fichier autorisé s'ajoute dans `filtre.rs` avec ses tests.
- **Salons vocaux** : un salon `cinema` est aussi un salon vocal (LiveKit). Toute règle propre au vocal teste `servers::est_vocal` (backend, `kind = ANY($n)` lié à `servers::TYPES_VOCAUX` en SQL) et `isVoiceKind` (front), jamais `kind == 'voice'` seul.
- **Synchronisation (regarder ensemble, musique, vocal)** : aucun comportement ne dépend d'un délai fixe (`setTimeout`, pause supposée suffisante) ;
  passer par des états explicites et des évènements.
