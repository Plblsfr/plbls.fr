# Migration du backend Go vers Rust : décisions

Ticket : EST-196. Réécriture du backend Go (~22 000 lignes, 175 routes, 37 migrations) en Rust, avec le principe du frontend : un cœur et des blocs, un bloc = un crate autonome. Comportement identique à la production (« ISO prod »). Bascule **big-bang**, sans coexistence Go/Rust ni bascule route par route. Ce document ne modifie aucun code.

## 1. Stack et version de Rust
- tokio, axum (WebSocket inclus), sqlx (rustls), tower / tower-http, serde, tracing, reqwest (rustls). cargo-deny en CI.
- Version de Rust : dernière stable au moment d'EST-197, épinglée dans `rust-toolchain.toml`. La liste exacte des crates et leurs versions sont choisies dans EST-197.

## 2. Structure du workspace
- Workspace Cargo : crates `core`, `block`, `estompe-api` (binaire), et un crate par bloc `blocks/<nom>` (gifs, musique, vocal, regarder-ensemble, transcription, videos, apercu-liens, notifications, emojis, avatars, badges, signalements, activite-statut).
- Règles de dépendance :
  - `core` ne dépend d'aucun bloc ;
  - un bloc dépend de `core` et de `block`, et d'un autre bloc seulement s'il le déclare dans `Needs` (comme `internal/block/block.go`) ;
  - `estompe-api` assemble tout ;
  - aucune dépendance circulaire.

## 3. Dossiers
Le Go est déplacé dans `backend-go/`, le Rust vit dans `backend/`. Le déplacement est inclus dans EST-197 (git mv, chemins de `image.yml`, `version.yml`, `changelog.yml`, `AGENTS.md`, READMEs, Dockerfile, commandes de vérification).

## 4. PostgreSQL et migrations
- sqlx **sans** vérification à la compilation : SQL écrit à la main, preuve par tests testcontainers.
- Les 37 fichiers de migration sont repris tels quels, sans renumérotation. Les doublons 026 et 031 sont sans danger : le suivi se fait par nom de fichier complet dans `schema_migrations(name text PRIMARY KEY)`, déjà en prod. Le moteur Rust garde ce suivi, l'ordre lexicographique des noms et le verrou consultatif 7001.

## 5. Image Docker
musl statique sur distroless static nonroot (uid 65532, `/data`, port 8080), même cible que l'image Go.

## 6. CI et version pendant la migration
Le Go (`backend-go/`) continue de livrer `api-v*` et de faire monter la version `api`. Le Rust (`backend/`) n'a que sa CI de tests, sans publication ni montée de version `api`, jusqu'à EST-227 : la bascule déplace la publication sur `backend/`.

## 7. Routes vocal du cœur
- `PUT /voice/{cid}/participants/{uid}/move` et `POST /voice/{cid}/participants/{uid}/disconnect` passent dans le **bloc vocal** (EST-213), montées seulement si `BLOCK_VOCAL=true`.
- Le cœur (EST-205) définit une interface optionnelle « vocal », implémentée par le bloc `vocal`. La modération sort la personne du vocal à l'exclusion ou au bannissement seulement si le bloc est actif ; sans vocal, l'exclusion fonctionne et ne touche pas LiveKit. La forme du code est laissée à EST-205 / EST-213.
- Comportement sans vocal : ISO prod strict ; les écarts constatés deviennent des tickets séparés après la bascule.

## 8. « ISO prod » et preuve (EST-226)
Comparaison de : statut + corps JSON (clés, types, erreurs, ordre des listes, valeurs générées neutralisées) ; en-têtes utiles (Content-Type, CORS, Cache-Control, Set-Cookie, Retry-After) ; évènements WebSocket (type, charge, ordre, destinataires) ; état en base et fichiers sur disque après chaque scénario.

## 9. Bugs du Go découverts
L'ISO est la règle, la correction est l'exception : une correction n'est faite que si le propriétaire la valide, et elle est notée au rapport de parité comme « différence justifiée ». Elle ne doit jamais rendre la base ou les fichiers illisibles par le Go (rollback).

Écart de la modération (EST-205), à noter au rapport de parité :
- Fin d'un ban ou d'une exclusion (`expires_at`, `timeout_until`, `until` de `member.left` et `member.timeout`, libellé d'audit « jusqu'au … UTC ») : calculée par PostgreSQL au lieu de l'horloge de l'API, précision à la microseconde au lieu de la nanoseconde. Différence justifiée.

Écarts de configuration (EST-198), à noter au rapport de parité :
- `MAX_STORAGE_GB`, `MAX_USER_STORAGE_MB`, `MAX_VIDEO_SIZE_MB` : le Go décale sans contrôle (`gb << 30`) et déborde silencieusement ; le Rust refuse une valeur dont le résultat dépasse `i64` (« X doit être un entier >= 1 »). Différence justifiée.
- Variables obligatoires manquantes : le Go cite l'une d'elles au hasard (parcours d'une `map`) ; le Rust cite toujours la première dans l'ordre `DATABASE_URL`, `PERMISSION_TOKEN`, `LIVEKIT_URL`, `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`.
- Cache d'identité : la clé est le SHA-256 de la valeur seule, un cookie `Bearer abc` et un Bearer `abc` partagent l'entrée (comme le Go, `via_cookie` peut alors être faux) ; conservé pour l'ISO.
- Valeur invalide d'un `BLOCK_*` : `%q` du Go et `{:?}` du Rust s'accordent sur l'ASCII imprimable, pas forcément sur les caractères de contrôle.

Écarts des middlewares (EST-201), à noter au rapport de parité :
- Format `%g` du Go reproduit par `metrics::go_g` (relevé sur un vrai programme Go) pour les sommes de durées de `/metrics`.
- Durée des requêtes : mesurée jusqu'à l'en-tête de réponse (axum rend la main avant la fin du corps) ; le Go la mesure à la fin de `ServeHTTP`.
- Journal des 5xx : non écrit si le client coupe avant la fin de la requête (le Go utilise un contexte indépendant).
- Une panique de gestionnaire devient un 500 journalisé par `journal_erreurs`, couche extérieure à `politique_origine` : ce 500 n'a pas les en-têtes CORS (le Go les posait avant le gestionnaire).
- Le chemin inscrit dans `server_errors` est tronqué à 512 caractères.
- Messages (EST-206) : le JSON entrant est lu avec des clés en minuscules (`content`, `file_ids`, `reply_to`, `poll`, `pseudo`, `option_id`, `message_ids`) là où `encoding/json` ignore la casse. Différence justifiée.
- Messages privés (EST-206) : comme en Go, l'autre participant d'un message privé peut supprimer un message ou clore un sondage (`Conv::peut` toujours vrai). Défaut connu conservé par parité, à corriger après le gel (ticket à ouvrir par le propriétaire).
- Messages (EST-206) : `closes_at` entrant (RFC 3339 validé, comparé par PostgreSQL), `closes_at` de `poll.closed` et délai du mode lent viennent de l'horloge PostgreSQL, à la microseconde au lieu de la nanoseconde ; le `closes_at` entrant refuse en plus l'année 0 et les décalages au-delà de 15:59, que le Go accepte. Différence justifiée.
- Messages (EST-206) : `validEmoji` reprend `unicode.IsSymbol` avec la crate `unicode-properties` (version d'Unicode propre à la crate).
- Banc de parité (EST-206) : les trames `notification` (bloc notifications, EST-218) et `presence` (cause non analysée) sont écartées de la comparaison. `GET /admin/overview` : `version` vient de `ESTOMPE_REVISION` à la compilation (Rust) et non de la révision git lue par le binaire Go (`JUSTIFIEES`, EST-210).
- Recherche et épingles (EST-207) : en message privé, comme en Go, l'autre participant peut épingler ou désépingler n'importe quel message (`Conv::peut` toujours vrai), et `message.updated` est publié même si l'état ne change pas. Défauts connus conservés par parité.
- Recherche (EST-207) : les dates de `avant:`, `après:` et `pendant:` sont liées en secondes Unix (`to_timestamp`), car le cœur n'a pas de crate de dates ; sans effet sur les résultats. Un octet invalide dans `q` peut compter autrement qu'en Go (`decode_utf8_lossy`) ; cas non couvert.
- Recherche (EST-207) : la condition de texte `m.tsv @@ q OR EXISTS (… transcript_tsv @@ q)` empêche PostgreSQL d'utiliser l'index GIN `messages_tsv` (parcours de `messages_pkey`, ou séquentiel en tri par pertinence). Le Rust reprend le SQL du Go et a les mêmes plans (`recherche_memes_index_que_le_go`). Défaut connu conservé par parité, à corriger après le gel.
- Recherche (EST-207) : `a:` est validé par une recherche de sous-chaîne dans `" fichier image video lien sondage "`, comme en Go. `a:"image video"` est donc accepté, puis ignoré par la requête : le filtre disparaît sans erreur. Défaut connu conservé par parité, à corriger après le gel.
- État de lecture (EST-209) : `lecture_scenario_nominal` (sans annonces) ne relève aucun écart ; au contrat OpenAPI, `GET /unread` diffère comme `GET /dms` (notes de version du compte Estompe envoyées par le Go seul, EST-210), écart justifié. L'ordre de `channels` et `dms` de `GET /unread` n'est pas défini (pas d'`ORDER BY`, comme en Go).
- `updates.json` : serde est strict (casse des clés, `null`) là où `encoding/json` est tolérant.

## 10. Gel du Go
Règle dans `AGENTS.md` : plus de fonctionnalité ni de migration dans `backend-go/`, seulement des correctifs bloquants portés aussitôt en Rust. La CI échoue si un fichier est ajouté dans `backend-go/internal/store/migrations/`.

## 11. Bascule et rollback (à reporter dans EST-229)
- Coupure de 3 h maximum.
- Heure de bascule : 0 h.
- Décideur du go / no-go : le propriétaire.
- Période de stabilité avant suppression du Go : 7 jours.

## 12. Ordre des tickets
EST-197 socle → 198 config/blocs → 199 PostgreSQL → 200 identité → 201 middlewares / 202 hub → 203–210 cœur → 211–224 blocs → 225 OpenAPI → 226 parité → 229 rollback → 227 bascule → 228 documentation.

EST-205 (interface « vocal ») doit précéder EST-213 (bloc vocal qui l'implémente) : dépendance **EST-205 → EST-213** à ajouter. Pas de cycle : EST-213 dépend de 202 et 204, EST-205 de 204.

## Changements décidés par ticket

### EST-197
- Le Go est déplacé dans `backend-go/` (git mv, `image.yml`, `version.yml`, `changelog.yml`, `AGENTS.md`, READMEs, Dockerfile, commandes de vérification) ; le Rust est posé dans `backend/`.
- CI Rust : tests seulement, sans publication ni montée de version `api`.
- Règle de gel du Go dans `AGENTS.md` et contrôle CI sur `backend-go/internal/store/migrations/`.
- Version de Rust : dernière stable, épinglée dans `rust-toolchain.toml`.
- Image : remplacer « équivalent de l'image Go actuelle » par musl statique sur distroless nonroot.
- Extrait de ces décisions dans `backend/README.md`.

### EST-205
- Retirer de la liste des routes et de « À faire » `move` et `disconnect`.
- Le cœur définit l'interface optionnelle « vocal » et ne s'en sert qu'avec `BLOCK_VOCAL=true`.

### EST-213
- Ajouter à la liste des routes `PUT /voice/{cid}/participants/{uid}/move` et `POST /voice/{cid}/participants/{uid}/disconnect`.
- Implémenter l'interface définie par EST-205.
- Reprendre les tests de `voice_test.go` liés à ces routes.

### EST-229
- Coupure de 3 h maximum, heure 0 h, décideur : le propriétaire, période de stabilité de 7 jours (au lieu de la suggestion de 14).
- Retirer ces points de « À préciser avec le propriétaire ».

### Dépendances
- Ajouter EST-205 bloque EST-213.
- Vérifier : 197 → 198 → 199 → 200, 229 avant 227, 213 après 205, 205 avant 227, aucun cycle.
