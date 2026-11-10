# Plan : un seul front-end pour security, permission, estompe et pli

Plan demandé par l'issue #12. Ce document ne change rien au code : il fixe la cible, tranche les questions ouvertes et découpe
le travail en tickets. La refonte de la CI/CD qui en découle est suivie dans #13 (voir « Lien avec #13 »).

## 1. Point de départ (constaté dans le code)

| Projet | Front | Servi par | Accès à l'API | Session et CSRF |
|---|---|---|---|---|
| permission | `web/permission` : Svelte 5 + Vite, routage par hash, ~800 lignes | le backend Rust (`ServeDir` sur `static_dir`, étape `front` du Dockerfile) | `https://security.plbls.fr/api/permission` (en dur dans `src/api.ts`), CORS avec `credentials: include` | cookie du gateway, contrôle `Origin` du gateway |
| pli | `web/pli/public` : JS sans build (`app.js`, `read.js`, `crypto.js`, `s.html`) | le serveur Rust `pli` (fonction `statique`, `/s/<id>` → `s.html`), CSP stricte en dur (`app.rs`, `CSP`) | même origine (`/api/*` sur `pli.plbls.fr`) | cookie `.plbls.fr` lu par pli et relayé à `GET /auth/me` ; `same_origin` exige `Origin` = `Host` |
| estompe | `web/estompe` : Svelte 5 + Vite, cœur + blocs, routage par hash, ~21 000 lignes, PWA (`vite-plugin-pwa`) | image nginx `estompe-web` (`web/estompe/plateforme`), qui génère `config.js` et la CSP au démarrage (`40-estompe-config.sh`) et sert aussi `/updates/` (installeur Windows, APK, `latest.json`, `web.json`) | `API_URL` (sous-domaine propre), WebSocket, LiveKit | cookie `.plbls.fr` relayé par l'API à `/auth/me`, `ALLOWED_ORIGINS` (CORS + CSRF) |
| estompe natif | Tauri Windows et Android (`apps/estompe/{windows,android,crates}`) | `frontendDist: ../../../../web/estompe/dist`, build `--mode windows|android` qui remplace l'alias `@platform` | URLs lues au build (`VITE_API_URL`, `VITE_GATEWAY_URL`, `VITE_KEYCLOAK_ISSUER`) | Bearer (PKCE loopback), pas de cookie |
| security | aucun front ; quelques assets (logos, favicons) servis par `routes/assets.rs`, référencés par aucun projet du dépôt | — | — | cookie `__Secure-plbls_sid`, `Domain=.plbls.fr`, `HttpOnly`, `Secure`, `SameSite=Lax` ; `Origin` exigé pour les méthodes non sûres ; origines autorisées = `allowed_origin_suffix` (tout `*.plbls.fr` en https) |

`packages/ui` est copié dans `*/vendor/plbls-ui` par `scripts/sync-ui.mjs` (et vérifié par `npm run ui:check`). Les polices viennent
de Google Fonts pour permission et pli, de `@fontsource-variable/*` (auto-hébergées) pour estompe.

Constat utile : le cookie est **déjà** posé sur le domaine parent, et estompe a **déjà** une configuration lue à l'exécution
(`/config.js` généré au démarrage du conteneur). Le plan généralise ces deux mécanismes plutôt que d'en inventer d'autres.

## 2. Décisions déjà prises (reprises de l'issue)

- **Structure** : `web/plbls` (front unique) et `api/{security,permission,estompe,pli}` (API séparées, déjà en place). `packages/ui` devient
  une dépendance npm du workspace : fin du vendoring et de `scripts/sync-ui.mjs`.
- **Routage** : un domaine, plusieurs sous-domaines servis par le même front ; le module affiché dépend du sous-domaine.
- **Sous-domaines configurables** : domaine et sous-domaines en variables d'environnement, jamais en dur. Les mêmes variables
  alimentent le choix du module, les origines autorisées (CORS, CSP), le domaine du cookie et les redirections du gateway.
- **Session** : cookie du gateway posé sur le domaine parent : une connexion pour tous les modules.
- **Estompe natif** : à trancher (section 4.7).
- **Pli** : migration vers Svelte si possible, en gardant le chiffrement côté client et le lien de partage (section 4.8).

## 3. Structure cible

La structure du dépôt suit déjà le découpage par rôle : `api/<projet>` (API Rust), `web/<projet>` (fronts), `apps/` (apps natives),
`docs/<projet>`, `deploy/<projet>`, `scripts/<projet>`, `packages/`. Le front unique s'y ajoute sous `web/plbls`, à côté des anciens
fronts, qui disparaissent après la bascule de chaque module (T14) :

```
web/
  plbls/                       front unique (Svelte 5 + Vite), image ghcr.io/plblsfr/web
    index.html                 une seule page, charge /config.js puis src/main.ts
    src/
      main.ts                  lit la config, choisit le module d'après l'hôte, l'importe à la demande
      socle/                   commun à tous les modules (voir 4.3)
      modules/
        permission/            ex web/permission/src
        pli/                   ex web/pli/public, réécrit en Svelte
        estompe/               ex web/estompe/src (cœur, blocs, vues)
          plateformes/web/     ex web/estompe/plateforme/src
      natif.ts                 point d'entrée des apps Tauri : socle + module estompe, sans choix par l'hôte
    public/<module>/           logos, favicons, manifeste propres à chaque module
    Dockerfile                 nginx, contexte = racine du dépôt
    docker/
      generer-config.sh        unique générateur de config.js et des blocs server nginx (image et e2e)
      csp/<module>.conf        CSP de chaque module, avec des marqueurs remplacés au démarrage
    e2e/                       Playwright, un fichier par module + socle
apps/
  estompe/                     windows, android, crates (Tauri, hors workspace Cargo comme aujourd'hui)
api/
  security/  permission/       inchangés ; permission ne sert plus de front
  pli/                         ne sert plus web/pli/public
  estompe/
deploy/plbls/                  compose et variables PLBLS_* du service web
packages/
  ui/                          inchangé, consommé par "@plbls/ui": "*" dans web/plbls/package.json
  domaines/                    crate Rust plbls-domaines + cas de test partagés avec le socle TS (voir 4.1)
```

Les docs, skills, `changelog.d` et `.version` d'Estompe restent où ils sont (`docs/estompe`, racine du dépôt) ; le README du front
devient `web/plbls/src/modules/estompe/README.md`. `apps/estompe` garde son propre `Cargo.lock` et la toolchain stable.

## 4. Questions tranchées

### 4.1 Variables d'environnement

**Décision** : un jeu de variables `PLBLS_*`, identique pour tous les conteneurs (un seul fichier d'environnement partagé dans
Dokploy). Chaque service en dérive ce dont il a besoin avec la même règle, écrite une fois en Rust (`packages/domaines`, crate
`plbls-domaines`, utilisée par security, estompe et pli) et une fois en TypeScript (`web/plbls/src/socle/domaines.ts`), vérifiées
par les mêmes cas de test (`packages/domaines/cas.json`).

| Variable | Format | Défaut | Exemple prod | Rôle |
|---|---|---|---|---|
| `PLBLS_DOMAINE` | nom d'hôte en minuscules, au moins deux labels, sans schéma ni point final | obligatoire | `plbls.fr` | domaine du cookie, base de tous les hôtes |
| `PLBLS_HTTPS` | `1` ou `0` | `1` | `1` | schéma `https`/`wss`, attribut `Secure` |
| `PLBLS_PORT` | entier 1-65535 ou vide | vide | vide | port public commun ajouté aux origines (dev et e2e seulement) |
| `PLBLS_SOUS_DOMAINE_SECURITY` | un label DNS `[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?` | `security` | `security` | gateway OIDC |
| `PLBLS_SOUS_DOMAINE_PERMISSION` | label | `permission` | `permission` | module permission |
| `PLBLS_SOUS_DOMAINE_PLI` | label | `pli` | `pli` | module pli (et son API sous `/api`) |
| `PLBLS_SOUS_DOMAINE_ESTOMPE` | label | `estompe` | `estompe` | module estompe |
| `PLBLS_SOUS_DOMAINE_ESTOMPE_API` | label | obligatoire si estompe est servi | valeur actuelle de `API_PUBLIC_URL` | API Estompe (REST, WebSocket, médias) |
| `PLBLS_SOUS_DOMAINE_VOIX` | label ou vide | vide | `voice` | LiveKit (CSP `connect-src` d'estompe) |
| `PLBLS_MODULE_PAR_DEFAUT` | `permission`, `pli`, `estompe` ou vide | vide | vide | module affiché quand l'hôte ne correspond à aucun sous-domaine (dev sur `localhost`) |

Règles de dérivation (les mêmes partout) :

- hôte d'un module = `<label>.<PLBLS_DOMAINE>` ; origine = `http[s]://<hôte>[:PLBLS_PORT]` ;
- origines de confiance = l'apex `PLBLS_DOMAINE` et les sous-domaines des modules et du gateway, **liste fermée** (pas de suffixe) ;
- URL du gateway = origine de `security` ; API permission = `<gateway>/api/permission` ; API pli = même origine, `/api` ;
  API estompe = origine de `ESTOMPE_API` (et `ws[s]://` pour le temps réel) ; voix = `ws[s]://<voix>` ;
- les labels doivent être distincts deux à deux ; une valeur hors format fait échouer le démarrage du conteneur (elle finit dans du
  JavaScript et des en-têtes HTTP : même garde-fou que `40-estompe-config.sh` aujourd'hui).

Les services gardent leurs fichiers de config et leurs préfixes (`GATEWAY__…`, `PERMISSION__…`) pour tout le reste. Ce qui
dépendait du domaine en dur en est retiré : `server.public_url`, `server.default_redirect`, `server.allowed_origin_suffix` et
`session.cookie_domain` du gateway, `GATEWAY_URL` de pli et d'estompe, `ALLOWED_ORIGINS` et `CAIRN_ORIGIN` d'estompe. Une variable
explicite garde la priorité pendant la transition (retour arrière sans redéploiement de code), puis est supprimée.

**Lecture côté front, à l'exécution** : l'image `web` génère au démarrage `/config.js`
(`globalThis.__PLBLS__ = {domaine, https, port, sousDomaines: {...}, moduleParDefaut}`), servi en `Cache-Control: no-cache`
et chargé par un `<script src="/config.js">` avant le module principal. C'est le mécanisme d'estompe, éprouvé : synchrone, compatible
avec `script-src 'self'`, une seule image pour tous les environnements. Le front ne lit plus aucune variable `VITE_*`, sauf le
build natif (4.7), qui n'a pas de serveur pour lui servir `config.js`.

**Dev local** : `PLBLS_DOMAINE=plbls.localhost`, `PLBLS_HTTPS=0`, `PLBLS_PORT=5173`. Les navigateurs résolvent `*.localhost` vers la
boucle locale sans toucher à `/etc/hosts`. Un seul serveur Vite sur 5173 répond pour tous les hôtes ; un petit plugin Vite
(`socle/dev/hotes.ts`) relaie vers les services locaux les requêtes dont l'hôte est celui du gateway ou de l'API Estompe
(`DEV_CIBLE_SECURITY=http://127.0.0.1:8080`, `DEV_CIBLE_ESTOMPE_API=http://127.0.0.1:8081`), et vers pli celles de `pli.*/api`.
Tous les hôtes partagent donc le port 5173, ce qui garde une seule variable `PLBLS_PORT`. Le mode actuel de permission (en-têtes
d'identité signés par Vite, sans gateway) reste disponible dans ce plugin. `http://localhost:5173` avec
`PLBLS_MODULE_PAR_DEFAUT=pli` permet de travailler sur un module sans gateway.

**Tests e2e** : `PLBLS_DOMAINE=plbls.test` (TLD réservé), `PLBLS_HTTPS=0`, `PLBLS_PORT` = port de Playwright, Chromium lancé avec
`--host-resolver-rules=MAP *.plbls.test 127.0.0.1`. Le choix du module par l'hôte et le cookie `Domain=plbls.test` sont donc testés
pour de vrai. `config.js` est produit par le même `generer-config.sh` que l'image.

### 4.2 Conséquences du cookie sur le domaine parent

**Attributs retenus** : `__Secure-plbls_sid=<id>; Domain=<PLBLS_DOMAINE>; Path=/; Secure; HttpOnly; SameSite=Lax`.

- `Domain` vaut `PLBLS_DOMAINE` sans point initial (équivalent à `.plbls.fr` pour les navigateurs, RFC 6265).
- Préfixe `__Secure-` gardé ; `__Host-` est impossible (il interdit `Domain`).
- `SameSite=Lax` gardé : tous les sous-domaines sont du même site, `Strict` n'apporterait aucune protection entre eux et casserait
  la première navigation depuis un lien externe (mail, messagerie). `Lax` reste nécessaire au retour de Keycloak.
- `HttpOnly` : aucun JavaScript, d'aucun sous-domaine, ne lit le cookie. Le front sait s'il est connecté par `GET <gateway>/auth/me`.
- Cookie de login (`<nom>_login`, anti login-CSRF) : inchangé, propre à l'hôte du gateway, `Path=/auth`.
- Dev (`PLBLS_HTTPS=0`) : `cookie_secure=false` et nom sans préfixe `__Secure-` (déjà le cas avec `deploy/dev.env`).

**CSRF entre sous-domaines** : `SameSite` ne protège pas d'un sous-domaine compromis ou tiers, puisqu'il est du même site. La
protection repose donc sur l'en-tête `Origin`, comparé à la **liste fermée** des origines de confiance (4.1) :

1. gateway : `allowed_origin_suffix` est remplacé par cette liste (plus `extra_origins` pour les extensions) pour le CSRF
   (`auth::csrf_ok`), le CORS avec `credentials` (`main.rs`) et les redirections (`origin::safe_redirect`) ; une requête par cookie
   avec méthode non sûre et sans `Origin` reste refusée ;
2. API Estompe : `politique_origine` lit la même liste (origine du module estompe) au lieu d'`ALLOWED_ORIGINS` ;
3. pli : front et API sur le même hôte, `same_origin` (`Origin` = `Host`) reste la règle ;
4. toute requête qui modifie reste en `POST`/`PUT`/`PATCH`/`DELETE` (pli : `GET` ne consomme jamais).

**Sous-domaines de confiance** : tout hôte sous `PLBLS_DOMAINE` reçoit le cookie dans ses requêtes, y compris ceux qui ne sont pas
de notre code (stack média, Jellyfin, LiveKit, CDN de `@plbls/ui`, Cairn). Un tel service peut le journaliser ou le voler s'il est
compromis. Décision :

- Keycloak (`auth.<domaine>`) est de confiance (il émet l'identité), il garde le cookie ;
- les services de notre code (gateway, permission, pli, estompe et son API) sont de confiance ;
- pour tout autre hôte, le reverse proxy de `plbls.fr-infra` retire l'en-tête `Cookie` (Traefik : middleware `headers`,
  `customRequestHeaders: {Cookie: ""}`) ; un service tiers qui a besoin de ses propres cookies passe sur un autre domaine enregistrable ;
- un sous-domaine n'est jamais délégué à un tiers (DNS `CNAME` vers un hébergeur mutualisé, prise de contrôle de sous-domaine).

**Keycloak** : seule redirect URI du client `plbls-gateway` : `<origine security>/auth/callback` (inchangée). Les post-logout
redirect URIs passent du joker `https://*.plbls.fr/*` à la liste explicite des origines de modules, générée dans `plbls.fr-infra` à
partir des mêmes variables. Clients `plbls-desktop` et `plbls-mobile` inchangés. Aucun « web origin » n'est nécessaire : le gateway
est un client confidentiel côté serveur.

### 4.3 Découpage du code

**Décision** : une page, un point d'entrée, des modules chargés par `import()` selon l'hôte.

- `main.ts` : lit `__PLBLS__`, trouve le module dont l'hôte est `location.hostname` (sinon `moduleParDefaut`, sinon page « module
  inconnu » avec les liens vers les modules), puis `await import('./modules/<id>/index.ts')`. Vite en fait un chunk par module :
  une page pli ne télécharge ni LiveKit, ni mediapipe, ni le code d'estompe.
- Chaque module exporte `{ id, titre, composant, icones, pwa?: boolean }` ; il garde son propre routage (hash pour permission et
  estompe, chemin `/s/<id>` pour pli). Le socle n'impose pas de routeur.
- `socle/` : `config.ts` et `domaines.ts` (lecture et dérivation), `auth.ts` (URL de login, d'inscription et de logout du gateway,
  `me()`, pilote d'authentification remplaçable par le natif, comme `AuthDriver` d'estompe), `api.ts` (fetch avec
  `credentials: include`, description des erreurs du gateway reprise de permission), `theme.ts` (`@plbls/ui`), `layout/` (barre de
  navigation `plbls-navbar`, sélecteur de modules qui renvoie vers les autres sous-domaines, compte), `erreurs/`.
- Règle d'import, vérifiée en CI : un module importe le socle, jamais un autre module ; le socle n'importe que `svelte` et
  `@plbls/ui`. Un test de build lit le manifeste Vite et échoue si le graphe de pli contient un chunk d'estompe ou de permission.
- Polices auto-hébergées (`@fontsource-variable/*`, comme estompe) pour tous : plus de Google Fonts dans les CSP, aucune requête
  tierce sur la page de lecture de pli.
- Service worker : seul le module estompe enregistre la PWA (`virtual:pwa-register`, appelé par son `index.ts`) ; le manifeste est
  servi par hôte. Aucun service worker sur l'hôte de pli.

### 4.4 Service du front

**Décision** : une image dédiée `ghcr.io/plblsfr/web`, nginx (même base que `estompe-web` et le CDN de `@plbls/ui`), et non le gateway.

Pourquoi pas le gateway : il est le composant le plus sensible ; y ajouter le service de fichiers et une CSP par hôte mélange les
rôles, lie chaque livraison du front à une livraison du gateway et oblige `security.<domaine>` à répondre pour tous les hôtes.
Pourquoi nginx plutôt que Caddy : le TLS est déjà terminé par le reverse proxy de Dokploy, nginx est déjà utilisé et son script
d'entrée générant config et en-têtes existe (`40-estompe-config.sh`).

Au démarrage, `generer-config.sh` écrit `config.js` et un bloc `server` par module (`server_name <hôte>`), chacun avec ses en-têtes :

| Hôte | CSP (résumé) | Autres en-têtes |
|---|---|---|
| pli | celle d'aujourd'hui (`script-src 'self'`, `connect-src 'self'`, `form-action 'self' <gateway>`, `frame-ancestors 'none'`), sans Google Fonts | `Referrer-Policy: no-referrer`, `X-Robots-Tag: noindex` sur `/s/*` |
| permission | `script-src 'self'`, `connect-src 'self' <gateway>`, `form-action <gateway>` | `Referrer-Policy: strict-origin-when-cross-origin` |
| estompe | celle de `40-estompe-config.sh` (`'wasm-unsafe-eval'`, lecteurs vidéo, API, WebSocket, voix) | `Permissions-Policy` d'estompe |
| autre | page « module inconnu » | — |

Communs : `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `/assets/` en cache immuable d'un an, tout le reste en
`no-cache`, repli sur `index.html` (donc `/s/<id>` sur pli), `/healthz`.

Les API ne servent plus de fichiers statiques : permission perd son étape `front` et `static_dir`, pli perd `public/` et la
fonction `statique` (ses en-têtes de sécurité restent sur les réponses JSON). Cela n'arrive qu'après la bascule de chaque module (5).

Les `/updates/` d'estompe (installeur Windows, APK, `latest.json`, `android.json`) sortent de l'image web : ils grossiraient
l'image de tous les modules et lieraient sa livraison à celle des apps natives. Ils vont dans une petite image nginx
`estompe-updates`, publiée par le workflow de livraison native et routée par le reverse proxy sur
`Host(<estompe>) && PathPrefix(/updates/)` : l'URL d'auto-mise à jour déjà installée chez les utilisateurs (`tauri.conf.json`,
`https://estompe.plbls.fr/updates/latest.json`) ne change pas. `web.json` (version de la PWA) est produit par l'image web elle-même.

### 4.5 Versionnement et livraison

- **Front** : un seul tag `web-vX.Y.Z`, version dans `web/plbls/package.json`, image `web` publiée par `_image.yml` (`edge` sur
  `main`, `X.Y.Z` + `latest` sur tag). Un module n'a pas de version propre : le CHANGELOG de `web/plbls` regroupe les modules.
- **API** : tags inchangés (`security`, `permission`, `pli`, `estompe-api`) ; les images `permission` et `pli` ne contiennent
  plus de front.
- **estompe-app** : gardé pour les **seules** apps natives (installeur Windows, APK, image `estompe-updates`). L'image `estompe-web`
  n'est plus publiée après la bascule d'estompe ; `.version` d'Estompe perd la clé `web`, et `bump-version.mjs` compte
  `web/plbls/src/socle/**`, `web/plbls/src/modules/estompe/**` et `web/plbls/src/natif.ts` pour `windows` et `android`.
- **ui** : `ui-vX.Y.Z` reste, pour l'image CDN et la release GitHub (consommateurs externes au dépôt, thème Keycloak de
  `plbls.fr-infra`). `web/plbls` consomme toujours le code de `packages/ui` du même commit (dépendance de workspace) : pas de
  montée de version à faire dans le front. `ui:sync`, `ui:check` et `scripts/sync-ui.mjs` disparaissent.
- La liste des tags de `AGENTS.md` gagne `web`.

### 4.6 Migration progressive et coexistence

**Ordre** : permission → pli → estompe (celui de l'issue, confirmé) : du plus petit au plus gros, et du moins risqué au plus risqué.
permission est petit et réservé aux administrateurs ; pli est petit mais son code touche au chiffrement ; estompe est gros, a une
PWA, des apps natives et des utilisateurs quotidiens.

**Coexistence** : chaque module bascule seul, par une modification de route dans le reverse proxy (`<module>.<domaine>` vers le
service `web` au lieu de l'ancien conteneur). Les anciens fronts restent dans le dépôt et dans leurs images jusqu'au ticket de
nettoyage qui suit la bascule de chaque module : le retour arrière consiste à remettre l'ancienne route. Le service `web` répond dès
le premier module ; les modules pas encore basculés n'ont simplement pas de route vers lui. Les services ne perdent leur service
statique qu'après validation de la bascule. Les API sont déjà dans `api/*` (T15, fait avant le reste du plan) : aucun autre déplacement
de dossier n'est prévu, le front unique s'ajoute dans `web/plbls` sans toucher aux anciens fronts avant leur nettoyage (T14).

**Impact sur `plbls.fr-infra`** :

- Dokploy : nouveau service `web` (image `ghcr.io/plblsfr/web`, fichier d'environnement `PLBLS_*` partagé), webhook de livraison
  sur `web-v*` ; à la bascule de chaque module, route du sous-domaine vers `web`, et pour pli une route prioritaire
  `Host(<pli>) && PathPrefix(/api)` vers le service `pli` ; nouveau service `estompe-updates` ; retrait de `estompe-web`.
- Variables : `PLBLS_*` ajoutées à security, estompe-api, pli et web ; anciennes variables gardées pendant la transition, puis retirées.
- DNS : aucun nouveau sous-domaine (les noms actuels sont gardés) ; baisser le TTL avant chaque bascule pour un retour rapide.
- Keycloak : post-logout redirect URIs explicites (4.2) ; redirect URI du callback inchangée.
- Reverse proxy : retrait de l'en-tête `Cookie` vers les hôtes tiers (4.2), limite de corps de `pli/api` au moins égale à `MAX_BYTES`.

### 4.7 Apps natives Estompe (Tauri Windows et Android)

**Décision** : les inclure. Elles construisent le module estompe depuis `web/plbls`, via `src/natif.ts` ; il n'y a pas de front
Estompe séparé.

- Le front d'estompe est déjà conçu pour cela : le code par plateforme passe par l'alias `@platform` choisi par le mode Vite. Garder
  un front séparé obligerait à dupliquer 21 000 lignes, ou à faire d'estompe une bibliothèque consommée par deux fronts : plus de
  travail que d'ajouter un point d'entrée.
- `web/plbls` : `vite build --mode windows|android` prend `natif.ts` comme entrée (socle + module estompe importé statiquement, sans
  choix par l'hôte) et sort dans `dist-windows`/`dist-android`. Les autres modules ne sont pas atteignables depuis cette entrée et
  ne sont pas dans le paquet. Les plateformes `windows` et `android` (TS) restent avec le code Tauri dans `apps/estompe`.
- Configuration native lue au build (`VITE_*`) : seule exception à la lecture à l'exécution, car une app installée n'a pas de serveur
  pour lui servir `config.js`. Les valeurs sont dérivées des mêmes `PLBLS_*` par `vite.config.ts`.
- `tauri.conf.json` : `frontendDist: ../../web/dist-<plateforme>`, `beforeBuildCommand` qui appelle le build de `web/plbls`.
- Coût accepté : un changement du socle ou du module estompe fait monter la version des apps natives (4.5) ; un changement d'un
  autre module ne la touche pas.

### 4.8 Pli en Svelte : ce qui bloquerait

Rien de bloquant n'a été trouvé dans le code ; la migration est possible aux conditions suivantes, chacune devenant un critère du
ticket :

1. **Format du chiffré inchangé** : `crypto.js` est repris tel quel (passé en TS, même algorithme, mêmes données authentifiées
   `pli/1`) ; un test déchiffre un partage produit par la version actuelle (fixture) et inversement.
2. **Liens existants** : `https://<pli>/s/<id>#<clé>` reste valide (repli `index.html` sur `/s/*`, route `/s/:id` du module) ; la clé
   reste dans le fragment, n'est jamais lue avant la vérification `validKey`, et quitte la barre d'adresse par `history.replaceState`
   après lecture.
3. **Un `GET` ne consomme jamais** : seul le bouton « Ouvrir le message » envoie `POST /api/s/<id>/consume`.
4. **Surface de la page de lecture** : seul le JS du socle et de pli est chargé (test du manifeste, 4.3), aucune ressource tierce
   (polices auto-hébergées), CSP `script-src 'self'` sans `'wasm-unsafe-eval'`, pas de service worker, `Referrer-Policy: no-referrer`.
   Si l'une de ces conditions ne pouvait pas être tenue dans une page partagée avec les autres modules, ce serait le vrai blocage :
   le chiffrement de bout en bout ne vaut que si le JS servi sur l'hôte de pli est minimal et contrôlé.
5. **Même origine pour l'API** : `same_origin` de pli compare `Origin` à `Host`/`X-Forwarded-Host` ; la route `/api` de pli doit
   donc rester sur l'hôte de pli (4.6), sans quoi il faudrait du CORS et un contrôle d'origine par liste.

Conséquence assumée : une image `web` compromise compromet pli comme les autres modules, comme aujourd'hui un serveur `pli` compromis
(limite déjà écrite dans le README de pli).

## 5. Tickets

À créer dans GitHub une fois le plan validé. Chacun est une PR indépendante, sauf dépendance indiquée.

**T1. `chore(ui): consommer @plbls/ui en dépendance du workspace dans estompe et permission`**
- C1 : `web/estompe` et `web/permission` importent `@plbls/ui` (JS et CSS) ; leurs `src/vendor/plbls-ui` sont supprimés.
- C2 : `npm ci && npm run build` à la racine construit `packages/ui` avant les fronts ; `npm run dev` d'un front marche sur un dépôt neuf.
- C3 : `scripts/sync-ui.mjs` ne copie plus que vers `web/pli/public` ; `just lint`, `just check`, `just test` passent.

**T2. `feat(domaines): crate plbls-domaines et variables PLBLS_*`**
- C1 : crate `packages/domaines` membre du workspace, dérivation des hôtes et origines (4.1), validation des formats.
- C2 : `packages/domaines/cas.json` couvre prod, dev (`plbls.localhost:5173`), e2e (`plbls.test`), labels invalides ou en double.
- C3 : `cargo test -p plbls-domaines` passe ; clippy et `cargo deny` passent.

**T3. `feat(security): origines et cookie dérivés de PLBLS_*`** (dépend de T2)
- C1 : `allowed_origin_suffix` remplacé par la liste fermée ; un sous-domaine hors liste reçoit 403 sur un `POST` par cookie et pas d'`Access-Control-Allow-Origin`.
- C2 : `Domain`, `public_url`, `default_redirect` dérivés de `PLBLS_*` ; une variable `GATEWAY__…` explicite garde la priorité.
- C3 : tests de `origin.rs` et `auth.rs` mis à jour (hors liste refusé, apex accepté, `extra_origins` accepté) ; tests e2e Python de security passent.
- C4 : `docs/security/security.md` et `docs/security/keycloak.md` décrivent la liste fermée et les post-logout explicites.

**T4. `feat(estompe): origines de l'API dérivées de PLBLS_*`** et **`feat(pli): URL du gateway dérivée de PLBLS_*`** (dépendent de T2, deux PR)
- C1 : `ALLOWED_ORIGINS`/`CAIRN_ORIGIN` (estompe) et `GATEWAY_URL` (pli) dérivés, avec priorité à la variable explicite.
- C2 : tests existants verts ; un test par service prouve la dérivation.

**T5. `feat(web): socle web/plbls et image web`** (dépend de T2 pour les cas de test)
- C1 : `web/plbls` dans le workspace npm, `lint`, `check`, `test`, `build` ; `@plbls/ui` en dépendance ; polices auto-hébergées.
- C2 : `generer-config.sh` produit `config.js` et les blocs nginx ; refuse une valeur hors format (test shell ou vitest qui l'appelle).
- C3 : choix du module par l'hôte, `PLBLS_MODULE_PAR_DEFAUT`, page « module inconnu » ; tests unitaires de `domaines.ts` sur `cas.json`.
- C4 : test du manifeste Vite : aucun module n'importe un autre module.
- C5 : e2e Playwright sur `*.plbls.test` (host-resolver-rules) : deux hôtes affichent deux modules factices.
- C6 : image `web` construite en CI ; smoke test qui vérifie la CSP et `config.js` par en-tête `Host`.
- C7 : tag `web-vX.Y.Z` publie l'image (`_image.yml`) ; `AGENTS.md` liste le tag `web`.

**T6. `feat(web): module permission`** (dépend de T1, T5)
- C1 : `web/plbls/src/modules/permission` reprend toutes les pages ; l'URL de l'API vient du socle (plus de `security.plbls.fr` en dur).
- C2 : tests vitest et e2e de permission déplacés et verts (y compris le mode « via gateway »).
- C3 : l'ancien front reste construit et servi par l'image `permission` (pas encore de nettoyage).

**T7. `chore(infra): basculer permission sur l'image web`** (dans `plbls.fr-infra`, dépend de T3, T6)
- C1 : service `web` déployé avec `PLBLS_*` ; route `permission.<domaine>` vers `web`.
- C2 : connexion, liste des apps, attribution d'un rôle et déconnexion vérifiées en prod ; retour arrière documenté.
- C3 : post-logout redirect URIs Keycloak explicites.

**T8. `feat(web): module pli en Svelte`** (dépend de T5)
- C1 : dépôt texte et fichier, liste et révocation des partages, lecture unique, comme aujourd'hui.
- C2 : conditions 1 à 5 de 4.8 couvertes par des tests (fixture de compatibilité, `GET` sans consommation, manifeste, CSP et `Referrer-Policy` de l'hôte pli).
- C3 : `crypto.test.js` repris en TS et vert.

**T9. `chore(infra): basculer pli sur l'image web`** (dépend de T8)
- C1 : route `pli.<domaine>` vers `web`, `pli.<domaine>/api` vers `pli` (prioritaire).
- C2 : un lien créé avant la bascule s'ouvre après ; dépôt et lecture vérifiés en prod.

**T10. `feat(web): module estompe`** (dépend de T1, T5)
- C1 : `web/estompe/src` et `web/estompe/plateforme/src` déplacés dans `web/plbls/src/modules/estompe`, cœur et blocs inchangés.
- C2 : `config.ts` d'estompe lit le socle ; PWA enregistrée par le seul module estompe, manifeste par hôte.
- C3 : tests vitest et e2e `chat.spec.ts` verts ; CSP de l'hôte estompe identique à celle de `40-estompe-config.sh`.
- C4 : changelog d'Estompe renseigné.

**T11. `feat(estompe): apps natives construites depuis web/plbls`** (dépend de T10)
- C1 : `natif.ts`, modes `windows` et `android` ; `apps/estompe/{windows,android,crates}` construisent depuis `web/plbls`.
- C2 : `tauri.conf.json` pointe vers `web/plbls/dist-<plateforme>` ; tests de l'app Windows et du crate commun verts ; compilation Android (hors Android) verte.
- C3 : image `estompe-updates` publiée par `estompe-livraison-app` ; `bump-version.mjs` et `.version` sans clé `web`.
- C4 (non vérifiable en CI) : installation et mise à jour d'une app Windows et d'un APK depuis une version précédente.

**T12. `chore(infra): basculer estompe sur l'image web`** (dépend de T10, T11)
- C1 : route `estompe.<domaine>` vers `web`, `estompe.<domaine>/updates/` vers `estompe-updates` ; `estompe-web` retiré.
- C2 : PWA déjà installée mise à jour sans réinstallation ; app Windows trouve `latest.json`.

**T13. `chore(infra): retirer le cookie de session des hôtes tiers`** (`plbls.fr-infra`, indépendant)
- C1 : inventaire des sous-domaines de `plbls.fr` et de leur propriétaire.
- C2 : middleware qui retire `Cookie` sur chaque hôte tiers ; vérifié par une requête de test.

**T14. `chore: retirer les anciens fronts`** (après T7, T9, T12 ; une PR par module si plus simple)
- C1 : `web/permission`, `web/pli/public`, `web/estompe`, `web/estompe/plateforme` supprimés.
- C2 : permission sans étape `front` ni `static_dir`, pli sans `statique` ; leurs tests mis à jour.
- C3 : `scripts/sync-ui.mjs`, son test, `ui:sync`, `ui:check` et les exclusions `vendor/plbls-ui` de `biome.json` supprimés.
- C4 : anciennes variables (`GATEWAY_URL`, `ALLOWED_ORIGINS`, `API_URL`…) retirées des services et de `plbls.fr-infra`.

**T15. `chore: structure du dépôt par rôle`** (fait)
- C1 : `git mv` vers `api/`, `web/`, `apps/`, `docs/<projet>`, `deploy/<projet>`, `scripts/<projet>` (voir `README.md`).
- C2 : `Cargo.toml` (membres), `.dockerignore`, `Dockerfile.dockerignore`, Dockerfiles, workflows, `justfile`, `AGENTS.md` et skills d'Estompe mis à jour.

## 6. Lien avec #13 (refonte CI)

#13 peut avancer sans attendre ce plan : son `ci.yml` unique gagne un job `web` à T5 et perd les jobs des anciens fronts à T14.
T15 (déplacements) a été fait après #13 : les chemins n'ont été réécrits qu'une fois. Les contrôles obligatoires du ruleset
(`scripts/configurer-github.sh`) suivent : `web` s'ajoute (ou est couvert par le contrôle unique de #13), `ui:check` disparaît.

## 7. Risques

| Risque | Effet | Parade |
|---|---|---|
| Un hôte tiers sous le domaine reçoit le cookie | vol de session | T13 ; aucun sous-domaine délégué à un tiers |
| Une seule image pour tous les fronts | une livraison ratée casse tous les modules | smoke test par hôte en CI, image précédente par tag, bascule module par module |
| Le socle tire du code d'un module dans pli | surface de la page de lecture agrandie | règle d'import et test du manifeste (4.3) |
| Erreur dans la CSP générée par hôte | module cassé, ou trop permissif | CSP comparées à l'existant dans le smoke test de T5, T8, T10 |
| PWA d'estompe : ancien service worker en cache sur `estompe.<domaine>` | ancienne version servie après bascule | même `navigateFallback`, `web.json` produit par l'image, vérification C2 de T12 |
| Les apps natives montent de version à chaque changement du socle | livraisons natives plus fréquentes | `bump-version.mjs` ne compte que le socle, estompe et `natif.ts` |
| `*.localhost` mal géré par un navigateur en dev | dev local bloqué | `PLBLS_MODULE_PAR_DEFAUT` sur `localhost` ; e2e sous Chromium avec host-resolver-rules |
| Double travail avec #13 | workflows réécrits deux fois | T15 après #13 ; T5 ajoute un job, pas un workflow |
| Variables anciennes et nouvelles en même temps | configuration ambiguë | priorité claire à la variable explicite, retrait daté en T14 |
| Bascule DNS ou route lente à revenir | indisponibilité prolongée | TTL bas avant bascule, retour arrière par route, anciennes images gardées |
