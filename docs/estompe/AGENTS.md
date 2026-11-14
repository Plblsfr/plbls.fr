# AGENTS.md — consignes pour les agents

Estompe : messagerie écrite et vocale (messages privés, serveurs, salons texte et vocaux). Backend Rust, frontend Svelte 5
(web, PWA, apps Windows et Android en Tauri), vocal LiveKit. Lire d'abord `docs/estompe/README.md`, `api/estompe/README.md` (dont « Ajouter un bloc ») et `web/estompe/README.md`.

Chemins : tous donnés depuis la racine du dépôt (Estompe est réparti entre `api/estompe`, `web/estompe`, `apps/estompe`, `docs/estompe`,
`deploy/estompe` et `scripts/estompe` ; `.version`, `CHANGELOG.md`, `changelog.d/` et `agents/skills/` sont à la racine).

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
| `.ai/GH-<n>/` | fichiers de travail d'un ticket (issue GitHub `#<n>`) : `brief.md`, `plan.md` (revues comprises), `verify.md` |

- Les deux sont à la racine du dépôt principal (parent de `git rev-parse --path-format=absolute --git-common-dir`), jamais dans un worktree.
- Ils sont exclus par `.git/info/exclude` (lignes `.ai/` et `.worktree/`), pas par `.gitignore`.
- `.ai/` n'est pas partagé : ce qui doit être connu de tous (décisions, critères, lien de la PR) va aussi en commentaire de l'issue GitHub.

## Skills et cycle d'un ticket

Guide pas à pas : `docs/estompe/ticket-claude-code.md`. Les skills sont versionnés dans `agents/skills/<nom>/SKILL.md` ; `.claude/skills` y pointe pour Claude Code. Un autre agent lit ces fichiers comme des procédures.

| Skill | Produit | Effet |
|---|---|---|
| `/estompe-grill-me <n>` | `.ai/GH-<n>/brief.md` | questions jusqu'à lever toute ambiguïté ; issue assignée (« en cours ») |
| `/estompe-plan <n>` | `.ai/GH-<n>/plan.md` | branche, étapes, tests, couverture de chaque critère |
| `/estompe-implement <n>` | worktree, commits locaux, `.ai/GH-<n>/verify.md` | suit le plan (ou les corrections), lance `scripts/estompe/check.sh`, prouve chaque critère |
| `/estompe-code-review <n>` | section « Revue N » de `plan.md` | revue 1 : trois relecteurs filtrés ; revue 2 : corrections seules ; rapport seul |
| `/estompe-pr <n>` | `changelog.d/<slug-court>.md`, push, PR (`Closes #<n>`) | commentaire sur l'issue, puis suppression du worktree |
| `/estompe-release` | release, tags, release GitHub | issues livrées commentées ; rouvertes s'il reste un critère non vérifié |

Règles du cycle :
- Un ticket est une issue GitHub de `Plblsfr/plbls.fr`, désignée par `#<n>` ; on la lit avec `gh issue view <n> --repo Plblsfr/plbls.fr --comments` et on la commente avec `gh issue comment`.
- Les critères de vérification du brief sont numérotés `C1`, `C2`… et suivis jusqu'à la release.
- Boucle courte : `implement` → `code-review` (revue 1) → si besoin `implement` (corrections) → `code-review` (revue 2, corrections seules) → `mr`.
  **Deux revues au plus** : seuls les bloquants et importants prouvés bloquent ; les mineurs vont dans la PR, tout nouveau défaut hors diff devient une issue.
  `/estompe-pr` relance `scripts/estompe/check.sh` lui-même et accepte une revue antérieure si les commits suivants ne sont que ses corrections.
- Après la création de la PR, `/estompe-pr` supprime le worktree (`git worktree remove`) ; la branche reste. Pour corriger la PR, `/estompe-implement` le recrée depuis la branche.
- L'issue se ferme à la fusion de sa PR (`Closes #<n>`). À la release, `/estompe-release` la commente avec le tag ; s'il reste un critère
  non vérifié, il la rouvre avec la liste de ce qui reste à vérifier. Un critère qui dépend d'un appareil réel reste « non vérifié » tant que la personne qui l'a n'a pas confirmé.
- Un bug se reproduit (test qui échoue) avant d'être corrigé. Sans reproduction possible, le dire sans affirmer de cause.
- Rester agnostique du système : aucune hypothèse sur un OS ou un appareil particulier.

## Nom de la branche = numéro de version

Le préfixe de la branche pilote la version de chaque élément (`api`, `web`, `windows`, `android`), stockée dans `.version` et mise à jour
par la CI à la fusion (`scripts/estompe/bump-version.mjs`, job `estompe-version` de `livraison.yml`) :

| Branche | Effet à la fusion dans `main` |
|---|---|
| `feat/…` | version **minor** +1 des éléments concernés (0.2.3 → 0.3.0) |
| `fix/…`, `chore/…`, `docs/…`, `ci/…`, tout autre | version **patch** +1 des éléments concernés (0.2.3 → 0.2.4) |
| `release/…` | **aucun effet** : réservé à la PR qui prépare une release |
| majeure | **à la main** : modifier la valeur dans `.version` |

Détails :
- Éléments concernés = dossiers modifiés par la fusion : `api/estompe/` → api ; `web/estompe/plateforme/` → web ; `apps/estompe/windows/` → windows ;
  `apps/estompe/android/` → android ; le reste de `web/estompe/` → web, windows et android ; `apps/estompe/crates/` → windows et android.
  Une fusion qui ne touche que `docs/`, `scripts/`, la CI… ne change rien.
- À chaque fusion, la CI recalcule `.version` (toutes les fusions depuis la dernière modification de `.version` sur `main`) et met à jour une PR
  unique `ci/version` (« chore(version) ») : `main` est protégée, la CI n'y pousse pas. Cette PR ne modifie que `.version` : dans `ci.yml`, seul le job
  `version-seule` tourne et vérifie qu'aucun autre fichier ne change. Fusionner cette PR (« merge ») avant de poser un tag de livraison.
  Ne pas modifier `.version` dans une autre PR, sauf pour une version majeure (les fusions pas encore comptées repartent alors de cette valeur).
  Au build, `bump-version.mjs --apply` recopie les versions de `windows` et `android` dans `tauri.conf.json`, `Cargo.toml` et `Cargo.lock`
  (l'espace de travail seulement) ; l'app ne propose la mise à jour que si la version publiée a augmenté.
- Le nom de la branche est lu dans le message du commit de fusion de GitHub (« … from feat/x into main »). **Fusionner en
  « merge » classique** : en squash ou rebase, le nom est perdu et la version monte en patch.
- Choisir le préfixe selon l'effet voulu : une fonctionnalité visible par les utilisateurs → `feat/`.

## Release et livraison

Branche `release/…`, `changelog.json`, forçage de mise à jour, tags `estompe-api-v*` et `estompe-app-v*` : voir `docs/estompe/release.md` (lu par `/estompe-release`).
Ne jamais écraser ni supprimer un tag existant ; pousser un tag seul (`push origin <tag>`), jamais `--tags`.

## Git et GitHub

- Dépôt : monorepo `https://github.com/Plblsfr/plbls.fr`, Estompe est réparti entre `api/estompe/`, `web/estompe/`, `apps/estompe/`, `docs/estompe/`, `deploy/estompe/` et `scripts/estompe/`. Les tags sont préfixés : `estompe-api-v*`, `estompe-app-v*`.
- **Seul `gh` parle à GitHub** (PR, issues, releases…), ou à défaut les outils MCP GitHub de la session pour lire et commenter une issue (`issue_read`, `add_issue_comment`). Interdits : `curl`/`wget` vers `api.github.com`, appel à l'API à la main,
  bibliothèque HTTP. Les règles `deny` de `.claude/settings.json` le font respecter.
- `gh` doit être connecté : `gh auth status` doit répondre. Sinon, s'arrêter et le dire à l'utilisateur.
- Le transport git (`fetch`, `pull`, `push`, tags) passe par `git` avec l'authentification de `gh`. Ne jamais forcer un push.
- PR : `gh pr create --base main --head <branche> --title "<titre>" --body "<corps>"`.
  Corps : contenu, critères et vérifications, et ce qui n'a pas été vérifié.
- Pousser et ouvrir une PR seulement via `/estompe-pr` ou `/estompe-release`, ou sur demande explicite de l'utilisateur.
- Messages de commit :
  - toujours `type(portée): résumé` (`feat`, `fix`, `chore`, `ci`, `docs`), avec le ticket ou sans ; sur un ticket : puis `type(portée): corrections de revue`, `type(portée): changelog` ;
  - release : `chore(release): api-vA.B.C app-vX.Y.Z` ;
  - la PR d'un ticket se termine par `Closes #<n>`.

## Vérifier avant de livrer

Depuis la racine du worktree, sans `cd … &&` :

```sh
scripts/estompe/check.sh
```

Le script lance, dans l'ordre : `TEST_DATABASE_URL` défini, `cargo fmt --check`, `clippy -D warnings` (tout le workspace racine), `cargo test` des crates d'Estompe, `cargo deny check`, `npm --prefix web/estompe run check`, `npm --prefix web/estompe test`. Il est la seule liste de contrôles : skills et CI s'y réfèrent.

Pendant le développement, du plus rapide au plus complet : `cargo check $(scripts/estompe/paquets-cargo.sh) --all-targets`, `cargo build $(scripts/estompe/paquets-cargo.sh)`, `cargo test $(scripts/estompe/paquets-cargo.sh) --no-run`, `cargo test $(scripts/estompe/paquets-cargo.sh)`. En local, `blaze` (indexable-inc) remplace `cargo` pour `build`, `check`, `run` et `test` (recompilation incrémentale en mémoire, voir `api/estompe/README.md`) ; `scripts/estompe/check.sh` et la CI restent sur `cargo` et la toolchain épinglée, qui font foi. Aucun linker à configurer : mesuré, mold n'apporte rien sur le linker par défaut (`api/estompe/README.md`, « Développement »).

- Rust : les crates de `api/estompe` sont membres du workspace Cargo racine du monorepo (`Cargo.toml`, un seul `Cargo.lock`, `deny.toml` et
  lints clippy communs avec `api/security`, `api/aegis` et `api/entracte`). La toolchain (1.99.0) est épinglée par `rust-toolchain.toml`,
  lu depuis tout le dépôt sauf `apps/estompe/{windows,android,crates}` (apps Tauri, hors workspace, sur la stable). `scripts/estompe/paquets-cargo.sh` donne les `-p` des crates d'Estompe.
  `cargo-deny` s'installe avec `cargo install cargo-deny --locked`.
- Plus de Docker en local. Les tests Rust utilisent le PostgreSQL de `TEST_DATABASE_URL` (compte superutilisateur, une base créée puis
  supprimée par test) : ils valident les migrations et les requêtes. Sans cette variable, ils essaient un conteneur (testcontainers, en CI)
  et sont sinon ignorés (« ignoré : Docker indisponible », test réussi) : vérifier la sortie.
  Toute base créée à côté de `testutil::postgres()` (modèle, copies) doit être rattachée par `BasePostgres::supprimer_aussi`, sinon elle reste sur le serveur.
- Front : `npm --prefix web/estompe run …` s'exécute dans `web/estompe/` et lit sa config ; `npx --prefix web/estompe …` lancé depuis la racine ne la lit pas.
- Dans un nouveau worktree, installer d'abord les dépendances front : `npm ci` (workspace npm à la racine du dépôt, un seul `package-lock.json`, Node du `.nvmrc` racine).
- Chaque PR ajoute un fichier `changelog.d/<slug-court>.md` (voir `changelog.d/README.md`) qui cite l'issue `(#<n>)`, sauf branches `ci/`, `docs/`,
  `release/` : la CI échoue sinon. Elle lance les tests de tout (backend, front, Rust).
- Dire clairement ce qui n'a **pas** été vérifié (rendu visuel, build Tauri/Windows, vrai partage d'écran…) dans la réponse et dans la PR.

## Tests

- Un changement de logique non triviale s'accompagne d'un test.
- Les nouveaux tests vont dans le fichier de test existant du code testé (Rust : module `#[cfg(test)]` du fichier testé ;
  front : `src/lib/<module>.test.ts`). Ne jamais créer de nouveau fichier ou de nouvelle classe de test quand un existe déjà pour ce code.
- Un test attend une condition, jamais un délai fixe.

## Architecture

- `api/estompe/` (Rust) : crates `estompe-api` (binaire), `core`, `block` et
  `blocks/<nom>` (un crate par bloc, mêmes noms que les blocs frontend). `core` ne dépend d'aucun bloc ; un bloc dépend de `core`, de `block`
  et d'un autre bloc seulement s'il le déclare dans `Needs` ; `estompe-api` assemble tout ; aucune dépendance circulaire.
- **Blocs** : un cœur et des fonctionnalités optionnelles. Backend Rust : un crate `api/estompe/blocks/<nom>` par bloc, déclaré dans `block_list()`
  (`api/estompe/estompe-api/src/main.rs`) avec son nom et ses dépendances (`needs`) ; `BLOCK_<NOM>=false` le désactive (routes absentes) ;
  le serveur annonce les blocs actifs dans `blocks` de `GET /me`. Frontend : `web/estompe/src/blocks/<nom>/` avec les mêmes noms (`registry.ts`),
  `app.on('<nom>')` masque un bloc inactif. Une nouvelle fonctionnalité optionnelle = un nouveau bloc des deux côtés, documenté dans
  `deploy/estompe/.env.example`. **Procédure complète : `api/estompe/README.md`, section « Ajouter un bloc ».**
- `web/estompe/` : code commun à toutes les applications. `src/components` et `src/views` : cœur (Svelte 5, runes) ; `src/lib` : état
  (`store.svelte.ts`, `voice.svelte.ts`, `prefs.svelte.ts`, `api.ts`) et interface `Platform` (`platform.ts`). UI : `plbls-ui` embarquée
  dans `src/vendor/plbls-ui`.
- Une plateforme = un dossier : `web/estompe/plateforme/` (image nginx, `docker/`), `apps/estompe/windows/` et `apps/estompe/android/` (coques Tauri : `src/platform.ts`,
  `src-tauri/`, `scripts/`). `apps/estompe/crates/core/` : code Rust et TS commun aux deux apps Tauri. Le frontend ne teste jamais la plateforme
  (`isDesktop`, agent) : il appelle `platform` ; un comportement propre à une plateforme s'ajoute dans son dossier (voir `web/estompe/README.md`).
- `.version` (racine du dépôt) : version de chaque élément (api, web, windows, android).
- `docs/estompe/` : `openapi.yaml` (REST) et `realtime.md` (évènements WebSocket). **Toute nouvelle route ou nouvel évènement est documenté ici.**
- Permissions : toute nouvelle route est rattachée à une permission de `api/estompe/core/src/catalogue.rs` (ou, avec un motif, à `HORS_PERMISSION`), et tout nouveau droit de serveur est plafonné par une permission ; les tests `estompe-api` et `estompe-core` l'imposent. Après un changement du catalogue : `AEGIS_DOC=1 cargo test -p estompe-core catalogue::tests::documentation_a_jour` régénère `docs/aegis/fonctionnalites.md`.
- Déploiement : services `estompe-*` et `livekit` de `deploy/docker-compose.yml` (voir `deploy/README.md`, « Variables ») : `estompe-api` charge tout le `.env`, `estompe-web` et `livekit` ne reçoivent que les variables de leur section `environment` (interpolées `${VAR}`) ; la base est `estompe` sur le PostgreSQL partagé (`postgres-main`), le Redis est le service `redis` commun. Nouvelle variable : valeur non secrète dans `plbls.env` (section Estompe), secret dans `deploy/estompe/.env.example` (valeur dans `secret/estompe` de Vault, référence `${{vault.plbls.estompe:CLE}}`), jamais en valeur dans le compose ; une variable lue par `estompe-web` ou `livekit` s'ajoute aussi à sa section `environment` ; aucun nom déjà utilisé par un autre projet de la stack, sauf les communes (`DATABASE_*`, `REDIS_*`, `VAPID_*`…).
- CI (détail : `docs/estompe/ci.md`) : en PR, `ci.yml` à la racine lance `rust-lint` et `rust-test` (API Rust : fmt, clippy, cargo-deny, tests avec PostgreSQL),
  `estompe-api-image` (build et démarrage de l'image), `front` (audit, types, tests, build), `estompe-tauri` (tests Rust des apps) et `changelog`
  (entrée `changelog.d/` exigée), chacun seulement si la PR touche ses chemins. Après fusion, `livraison.yml` calcule `.version` (`estompe-version`),
  publie l'image `:edge` de l'API si `api/estompe/` a changé et l'image de job de `deploy/estompe/ci/` ; tag `estompe-api-v*` : image versionnée de l'API.
  `estompe-livraison-app.yml` (tag `estompe-app-v*` ou lancement manuel) : web, installeur Windows, APK.

## Conventions

- **Migrations** : ajouter `NNN_nom.sql` (numéro suivant) dans `api/estompe/core/migrations/`. Ne jamais modifier une migration déjà fusionnée dans `main` : en ajouter une nouvelle.
- **Temps réel** : une modification d'état visible par d'autres passe par un évènement WebSocket publié par l'API
  (`publishMessage`) et géré dans `store.svelte.ts` (`onEvent`) ; prévoir le test dans `store.test.ts`.
- **Design** : l'interface suit `plbls-ui` ; pas de refonte visuelle avant la v2 de plbls-ui (bêta ouverte).
- **Sécurité** : valider toute entrée côté serveur (longueurs, formats, droits : auteur du message, membre du serveur) ;
  le client n'est jamais une source de confiance (ex. plafonds du partage d'écran annoncés par le serveur).
- **Desktop (Tauri)** : le webview n'envoie pas les mêmes en-têtes qu'un navigateur ; les appels à Keycloak et au gateway
  passent par Rust (`native_fetch`). La CSP est dans `apps/estompe/windows/src-tauri/tauri.conf.json` (et `apps/estompe/android/`).
- Code minimal : réutiliser l'existant (helpers, composants, patterns) avant d'ajouter ; pas de dépendance pour quelques lignes.

## Pièges connus

Quand un ticket révèle un piège, la PR l'ajoute ici.

- **Config cargo** : cargo lit `.cargo/config.toml` depuis le dossier courant et ses parents, jamais depuis `--manifest-path` : `scripts/estompe/check.sh` lance cargo depuis la racine du monorepo, un fichier dans `api/estompe/` y est ignoré.
- **Workspace racine** : une dépendance ajoutée ou montée dans `Cargo.toml` racine touche aussi security et permission ; leurs CI se déclenchent sur ce fichier, `Cargo.lock`, `rust-toolchain.toml` et `deny.toml`.
- **Champs tactiles** : iOS zoome sur un champ sous 16 px et ne dézoome pas ; la règle `@media (pointer: coarse)` d’`app.css` les force à 16 px, ne jamais poser `maximum-scale` ni `user-scalable`.
- **Lecteur YouTube** : toujours l'iframe officielle avec `referrerpolicy="strict-origin-when-cross-origin"` (sans `Referer`, YouTube
  répond par l'erreur 153). L'origine de la webview Tauri est `https://tauri.localhost` : tester la lecture dans l'app, pas seulement dans un navigateur.
- **Lecteurs tiers (YouTube, Vimeo, Twitch, Dailymotion)** : une iframe pilotée par `postMessage` (`providers.ts`), jamais le script du fournisseur dans la page ;
  `script-src` reste `'self'` (web et Tauri). Un message n'est lu que s'il vient de l'origine du lecteur et de la fenêtre de son iframe. Twitch n'a pas de protocole
  `postMessage` documenté : rediffusions, directs et clips ne synchronisent que le lancement (`sync: 'start'`).
- **Lecture automatique** : le navigateur ou la webview peut la refuser (`onAutoplayBlocked`) ; prévoir le bouton « Activer le son », ne jamais supposer que `play()` aboutit.
- **Empilement** : un menu ouvert dans un modal doit s'afficher au-dessus de lui sans être coupé par un `overflow` masqué ; réutiliser
  `ContextMenu` / `Modal` plutôt qu'un menu local, et ne pas régler un souci d'empilement par un `z-index` arbitraire.
- **Composants partagés obligatoires** : `Modal`, `ConfirmModal`, `ContextMenu`, `Icon`, `Avatar`… avant d'en écrire un autre ; jamais `window.confirm`, `alert` ni `prompt`.
- **Salon cinéma** : les titres sont ceux d'Entracte (`ENTRACTE_INTERNE_URL`, `ENTRACTE_SERVICE_TOKEN`) ; Estompe ne sert aucun flux ni fichier vidéo et ne garde aucune progression. Il vérifie le titre et le droit de l'hôte de serveur à serveur (`GET /internal/titles/{id}?sub=`) ; au départ de l'hôte, le suivant est un participant à qui Entracte ouvre le titre.
- **Diffusion sur une télé** (`docs/entracte/transcodage.md`) : uniquement l'API Remote Playback du navigateur (`video.remote`), jamais le script du SDK Google Cast (`script-src` reste `'self'`). Une source MSE (hls.js) ne se diffuse pas : le flux est rechargé dans le lecteur natif avant `prompt()`, qui exige un geste récent. La télé lit l'URL elle-même : sans cookie ni en-tête, et en CORS depuis sa propre origine.
- **Salons vocaux** : un salon `cinema` est aussi un salon vocal (LiveKit). Toute règle propre au vocal teste `servers::est_vocal` (backend, `kind = ANY($n)` lié à `servers::TYPES_VOCAUX` en SQL) et `isVoiceKind` (front), jamais `kind == 'voice'` seul.
- **Instant replay** (`docs/estompe/instant-replay.md`) : le moteur est derrière la feature `instant-replay` et doit compiler pour mingw et MSVC, et sous Linux pour ses tests ;
  tout appel Windows reste dans `replay/win/`, la logique testable ailleurs. Un objet COM de Media Foundation n'est pas `Send` : il change de fil dans `Envoi`,
  repris par `prendre()` (un `let Envoi(x) = …` dans une fermeture ne capture que le champ, et la compilation échoue). Le crate déclare `rust-version = "1.77"` :
  clippy refuse les API plus récentes (`is_none_or`, `ErrorKind::StorageFull`…). Les commandes qui lisent le matériel ou le disque sont `#[tauri::command(async)]` :
  une commande synchrone tourne sur le fil principal. La galerie n'accepte qu'un chemin canonisé placé dans le dossier des clips.
- **Éditeur de clips** (`docs/estompe/editeur-clips.md`) : le ffmpeg embarqué est un build LGPL, sans `libx264` ; le seul encodeur H.264 logiciel garanti est
  `libopenh264`, et aucun `ffprobe` n'est embarqué (les flux se lisent dans la sortie de `ffmpeg -i`). Un export se coupe en images (`fps` puis
  `trim=start_frame/end_frame`), pas en secondes : couper la vidéo par le temps décale d'une image selon les arrondis. Un évènement de fin d'export peut arriver avant le
  retour de la commande qui l'a lancé : le front ne suit un export que par ses évènements. Le Chromium de Playwright ne lit ni H.264 ni AAC : un contrôle de
  l'aperçu dans le navigateur se fait avec un fichier VP9 et des pistes WAV. Un raccourci de l'éditeur ne doit pas se déclencher depuis un champ de saisie, mais
  une poignée (`role="slider"`) qui garde le focus après un glisser ne doit pas bloquer annuler et rétablir.
  Le texte d'un montage n'est pas rendu par `drawtext` : la webview le dessine (`dessinerTexte`) et l'export incruste l'image, aux dimensions exactes de la
  sortie ; changer le calcul de la sortie d'un côté (`dimensionsSortie`, `dimensions_sortie`) sans l'autre fait refuser l'export. Le ffmpeg embarqué n'a pas
  de `-filter_complex_script` : le graphe passe en argument, limité par la ligne de commande de Windows. Une entrée ffmpeg ne se relit pas en arrière : un
  passage placé avant un autre du même clip prend une entrée de plus (`planifier`), sinon ses images s'accumulent en mémoire. `drawbox` ne rend pas un pixel
  opaque sur un fond transparent sans `replace=1`. Une sortie au rapport imposé se calcule en multiples pairs du rapport, pas en arrondissant largeur et hauteur.
  Dans la vue, seule la vidéo du segment lu est « active » : les évènements des autres vidéos (pause au changement de clip) sont ignorés.
- **Synchronisation (regarder ensemble, musique, vocal)** : aucun comportement ne dépend d'un délai fixe (`setTimeout`, pause supposée suffisante) ;
  passer par des états explicites et des évènements.
