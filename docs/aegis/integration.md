# Intégration

## Brancher une application

1. **Jeton** : une variable `AEGIS_TOKEN_<APPLICATION>` par application (`AEGIS_TOKEN_ESTOMPE`, `AEGIS_TOKEN_PLI`, `AEGIS_TOKEN_ENTRACTE`),
   déclarée dans le `.env.example` de l'application et lue aussi par aegis, qui en tire le nom de l'application (suffixe en minuscules,
   `_` devenant `-`) ; un jeton par application, comparé en temps constant, révocable séparément. Le jeton ne lit que les droits de son application (plus `aegis:admin`) et ne déclare que son application.
2. **Manifeste** au démarrage puis toutes les 10 minutes :

```http
PUT /v1/manifest
Authorization: Bearer <jeton d'estompe>

{"app": "estompe", "name": "Estompe",
 "resource_types": [{"key": "server", "name": "Serveur", "list_url": "http://estompe-api:8080/internal/aegis/resources/server"}],
 "permissions": [{"key": "vocal.screen", "category": "Vocal", "label": "Partage d'écran",
                  "description": "…", "resource_type": "server", "open_default": true}]}
```

3. **Droits d'un compte**, à mettre en cache selon `valid_until` :

```http
POST /v1/effective
Authorization: Bearer <jeton>

{"sub": "<sub Keycloak>", "app": "estompe"}
→ {"sub": "…", "generation": 3, "valid_until": "…", "permissions": {"estompe:gifs.use": {"allow": ["*"], "deny": []}}}
```

   Seules les permissions de l'application du jeton et celles d'aegis (`aegis:admin`) sont rendues ; `app` (facultatif) restreint à
   l'une des deux, une autre application répond 403. L'e-mail et le nom de l'annuaire viennent de la synchronisation Keycloak.

   Contrôle ponctuel : `POST /v1/check {"sub", "permission", "resource"?}` → `{"allowed": bool}`. Pour l'utilisateur connecté
   via la passerelle : `GET /v1/me/effective?app=estompe`. Erreur réseau : **refuser**.

4. **Invalidation** : s'abonner à `<prefix>:invalidation` sur le Redis partagé (`<prefix>` = `AEGIS_REDIS_PREFIX`, `aegis` par défaut) et vider son cache à chaque
   message (`compte` : ce `sub` ; `tout` : tout le cache). Un utilisateur Redis en lecture seule de ce canal suffit :
   `ACL SETUSER estompe on >… resetchannels &aegis:invalidation -@all +subscribe +ping`.

5. **Ressources** (si un type est déclaré) : `GET <list_url>?q=<texte>&ids=<id,…>` appelé par aegis avec le jeton de
   l'application, réponse `[{"id", "name"}]` (50 au plus). Les hôtes appelables sont limités par `AEGIS__RESSOURCES__ALLOWED_HOSTS` :
   un manifeste dont un `list_url` vise un autre hôte (ou n'est pas en `http`/`https`) est refusé en 400. Réponse lue en flux, 1 Mio au plus ; l'administration ne reçoit qu'un message
   générique en cas d'échec (`estompe injoignable`, `… a répondu une erreur`), le détail est dans les logs d'aegis.

## Configuration d'aegis

| Variable | Rôle |
| --- | --- |
| `DATABASE_URL`, `DATABASE_USER`, `DATABASE_PASSWORD` | Communes : serveur PostgreSQL, aegis rejoint la base `aegis` (migrations au démarrage) |
| `GATEWAY_IDENTITE_SECRET` | Commune : secret HMAC partagé avec la passerelle ; vide = refus de démarrer |
| `REDIS_URL`, `REDIS_PASSWORD`, `AEGIS_REDIS_PREFIX` | Communes : cache et pub/sub sur le Redis partagé (`REDIS_URL` vide = sans cache ni invalidation, les applications retombent sur leur TTL) |
| `AEGIS_TOKEN_<APPLICATION>` | Jeton de chaque application cliente (ci-dessus) |
| `AEGIS_ADMIN_ROLE` | Commune : rôle Keycloak super-admin (défaut `aegis-admin`) |
| `GATEWAY_INTERNE_URL`, `GATEWAY_INTERNE_TOKEN` | Communes : annuaire : `/internal/users` de la passerelle (avec `GATEWAY__INTERNAL__LIST_ALL=true`) |
| `AEGIS__ANNUAIRE__SYNC_MINUTES` | Période de synchronisation Keycloak (15 ; 0 = désactivée) |
| `AEGIS__RESSOURCES__ALLOWED_HOSTS` | Hôtes autorisés pour les `list_url`, séparés par des virgules (vide = aucun : tout manifeste avec un `list_url` est refusé) |
| `AEGIS__CACHE__TTL_SECS` | Durée maximale d'un droit en cache (600) |

Passerelle : route `/api/aegis` vers `http://aegis:8080` (`api/security/config/routes.toml`).

## Estompe

| Variable | Rôle |
| --- | --- |
| `AEGIS_URL` | URL interne d'aegis (défaut `http://aegis:8080`) |
| `AEGIS_TOKEN_ESTOMPE` | Obligatoire. Jeton d'Estompe (application `estompe` chez aegis) ; vérifie aussi les appels d'aegis vers `/internal/aegis/resources/*` |
| `REDIS_URL`, `REDIS_PASSWORD`, `AEGIS_REDIS_PREFIX` | Communes : abonnement aux invalidations |
| `AEGIS_ADMIN_ROLE` | Commune : rôle Keycloak qui ouvre Administration › accès sans compte Estompe (défaut `aegis-admin`) |
| `AEGIS_RESOURCES_URL` | URL par laquelle aegis joint Estompe (défaut `http://estompe-api:8080`) |

Estompe envoie son manifeste (permissions des blocs actifs), applique la garde du catalogue sur ses routes, plafonne les rôles
de serveur, renvoie `capabilities` dans `GET /me` (confort d'affichage) et relaie `/admin/acces/*` vers `/v1/admin/*` d'aegis
avec le justificatif de l'appelant : aegis reste seul juge. Détail : [fonctionnalites.md](fonctionnalites.md).

## Entracte

`AEGIS_URL` et `AEGIS_TOKEN_ENTRACTE` (application `entracte` chez aegis) : Entracte envoie son manifeste
(`entracte:access`, `entracte:request`, `entracte:admin`, fermées par défaut) et lit les droits par `POST /v1/effective` (cache borné par
`valid_until`, 30 s au plus). Sans aegis, toutes ses permissions sont refusées. Voir [docs/entracte](../entracte/README.md).

## Mise en service

1. Déployer aegis (base, Redis, route de la passerelle), avec les jetons d'Estompe et d'Entracte.
2. Donner le rôle Keycloak `aegis-admin` (et `user`) aux administrateurs.
3. Démarrer Estompe pour qu'il envoie son manifeste, puis configurer dans Administration › accès : un rôle `Membre` avec
   `estompe:access` (ou l'ajouter à « Tout le monde »), `entracte:access` aux bons comptes, les permissions `estompe:admin.*` aux administrateurs.
