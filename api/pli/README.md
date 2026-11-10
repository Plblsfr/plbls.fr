# pli.plbls.fr

Partage chiffré de bout en bout, **à lecture unique**. On dépose un texte ou un fichier, on obtient un lien ; la première personne qui l'ouvre lit le contenu, et il est effacé. Interface en [`@plbls/ui`](../../packages/ui) v2 (design « surligneur », thème clair/sombre automatique). Le logo est dans [`web/pli/public/logo/`](../../web/pli/public/logo) (envelope sur pavé rose, mot « pli » vectorisé, versions claire et sombre).

- **Dépôt** : réservé aux comptes connectés via **security.plbls.fr** *et* autorisés par **permission.plbls.fr** (`pli:deposit`).
- **Lecture** : ouverte à tout le monde, sans compte.

## Comment ça protège le contenu

1. Le navigateur de l'expéditeur génère une clé AES-256 aléatoire et chiffre le contenu (AES-GCM, WebCrypto). Le **nom, le type et la nature** (texte/fichier) sont dans la partie chiffrée.
2. Seul le chiffré part vers le serveur, qui ne voit que sa taille.
3. Le lien est `https://pli.plbls.fr/s/<id>#<clé>`. La clé est dans le **fragment** (`#…`) : le navigateur ne l'envoie jamais au serveur, ni aux journaux du proxy.
4. Le destinataire ouvre le lien, clique « Ouvrir le message » ; le navigateur demande le chiffré (`POST /api/s/<id>/consume`) puis le déchiffre localement.
5. La lecture est un `DELETE … RETURNING` unique en base : deux lecteurs simultanés, un seul obtient le contenu.

Choix qui en découlent :

- **Un `GET` ne consomme jamais** : seule la requête `POST` du bouton le fait. Les robots, aperçus de liens (Slack, messageries) et antivirus ne brûlent donc pas le lien. `GET /api/s/<id>` ne renvoie que la taille et l'expiration.
- Une fois la requête de lecture traitée, le message est perdu même si le déchiffrement échoue (clé tronquée, réseau coupé en route). La page vérifie la clé *avant* de consommer et le dit clairement après.
- L'adresse est retirée de la barre d'adresse (`history.replaceState`) après lecture.
- Les partages non lus expirent (1 h, 1 j, 7 j ou 30 j au choix, purge toutes les minutes). L'expéditeur voit ses partages en attente (taille, dates : jamais le contenu ni le lien) et peut les **révoquer**.
- Lecture publique : limitation à 60 requêtes/minute/IP (en mémoire, derrière le proxy via `X-Forwarded-For`), identifiants de 128 bits.
- CSP stricte (`script-src 'self'`, `frame-ancestors 'none'`), `Referrer-Policy: no-referrer`.

**Limites assumées** : quelqu'un qui a le lien a tout (comme pour tout lien secret : l'envoyer par un canal de confiance). Un serveur compromis pourrait servir un JavaScript piégé : le chiffrement protège contre la lecture de la base, des sauvegardes et des journaux, pas contre une page malveillante. Pas de mot de passe supplémentaire (peut s'ajouter plus tard : dériver la clé avec PBKDF2).

## Format du chiffré

`0x01 | IV (12 o) | AES-256-GCM( longueur de l'en-tête (4 o) | en-tête JSON | contenu )`, données authentifiées `pli/1`. Voir [`web/pli/public/crypto.js`](../../web/pli/public/crypto.js).

## Choix techniques

Serveur en **Rust** (axum, tokio, sqlx, serde, tracing), membre du workspace Cargo racine comme les autres backends : code dans ce dossier (`api/pli`), crate `pli`. Front sans build ([`web/pli/public`](../../web/pli/public)), servi par le serveur. Schéma Postgres appliqué au démarrage par les migrations sqlx de [`migrations/`](migrations), idempotentes (`IF NOT EXISTS`) : une base créée par l'ancien serveur Node est reprise telle quelle. Le chiffré est stocké en `BYTEA` (25 Mo max par défaut, `MAX_BYTES`).

Variables d'environnement : `PLI_DATABASE_URL` (obligatoire), `PORT` (8080), `GATEWAY_URL` (`https://security.plbls.fr`), `GATEWAY_INTERNAL_URL` (= `GATEWAY_URL`), `PERMISSION_URL` et `PERMISSION_SERVICE_TOKEN`, ou `AEGIS_URL` et `PLI_AEGIS_TOKEN` (sans eux, le dépôt répond 503), `DEPOSIT_PERMISSION` (`pli:deposit`), `MAX_BYTES`, `MAX_PENDING_PER_USER` (50), `RUST_LOG` (`info`).

## Connexion et permissions

Comme le hub : aucune route n'est ajoutée au gateway. Le cookie de session de security.plbls.fr (`Domain=.plbls.fr`) arrive aussi sur `pli.plbls.fr` ; le serveur le transmet à `GET /auth/me`, puis demande à permission si le compte a `pli:deposit` (token de service en lecture seule, cache 30 s, **refus en cas d'erreur**). Les requêtes qui modifient doivent porter un `Origin` égal à l'hôte de pli (anti-CSRF).

**À configurer une fois :**

- *permission.plbls.fr* : ajouter `pli:<token>` à `PERMISSION__SERVICES__TOKENS` (le même token dans `PERMISSION_SERVICE_TOKEN`, sans guillemets), puis : application `pli` (`POST /v1/apps {"key":"pli","name":"Pli"}`), permission `deposit` (`POST /v1/apps/pli/permissions`), rôle `depositor` contenant cette permission, et attribution aux comptes autorisés.
- *Base* (administrateur Postgres) : `CREATE USER pli WITH PASSWORD '…'; CREATE DATABASE pli OWNER pli;`
- *Reverse proxy* de `pli.plbls.fr` : transmettre `Host` (ou `X-Forwarded-Host`), le cookie et `X-Forwarded-For`. Prévoir une limite de corps au moins égale à `MAX_BYTES`.
- *Dépôt GitHub* : rien à configurer, la CI publie `ghcr.io/plblsfr/pli` avec `GITHUB_TOKEN` (`edge` sur `main`, `X.Y.Z` et `latest` sur un tag `pli-vX.Y.Z`).

## Lancer

En production : service `pli` de la stack [`deploy/docker-compose.yml`](../../deploy/docker-compose.yml), variables dans [`deploy/pli/.env.example`](../../deploy/pli/.env.example) (secrets dans `secret/pli` de Vault, voir [`deploy/README.md`](../../deploy/README.md)).

En local, depuis la racine du dépôt : `cargo run -p pli` avec ces variables exportées. L'image se construit depuis la racine : `docker build -f api/pli/Dockerfile .` (fichiers envoyés : [`Dockerfile.dockerignore`](Dockerfile.dockerignore), workspace Cargo et `web/pli/public/`).

Tests : `TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres cargo test -p pli` (une base jetable est créée puis supprimée par test ; sans cette variable, les tests passent par testcontainers et sont ignorés si Docker manque). Tests du front (`web/pli/public/*.test.js`) : `npm ci` à la racine du dépôt (workspace npm), puis `npm test --workspace web/pli` (Node 26, voir `.nvmrc`).

## Design system

`web/pli/public/vendor/plbls-ui/` contient `@plbls/ui` : copie générée depuis `packages/ui` par `npm run ui:sync` à la racine du dépôt, à ne pas modifier à la main (la CI échoue si elle diverge).
