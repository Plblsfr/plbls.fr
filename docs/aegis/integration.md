# Intégration

## Brancher une application

1. **Jeton** : déclarer l'application côté aegis, `AEGIS__SERVICES__TOKENS=estompe:<jeton>,pli:<jeton>` (un jeton par application,
   comparé en temps constant, révocable séparément). Le jeton ne lit que les droits de son application (plus `aegis:admin`) et ne déclare que son application.
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

4. **Invalidation** : s'abonner à `<prefix>:invalidation` sur le Redis d'aegis (`aegis` par défaut) et vider son cache à chaque
   message (`compte` : ce `sub` ; `tout` : tout le cache). Un utilisateur Redis en lecture seule de ce canal suffit :
   `ACL SETUSER estompe on >… resetchannels &aegis:invalidation -@all +subscribe +ping`.

5. **Ressources** (si un type est déclaré) : `GET <list_url>?q=<texte>&ids=<id,…>` appelé par aegis avec le jeton de
   l'application, réponse `[{"id", "name"}]` (50 au plus). Les hôtes appelables sont limités par `AEGIS__RESSOURCES__ALLOWED_HOSTS`.

## Configuration d'aegis

| Variable | Rôle |
| --- | --- |
| `AEGIS__DATABASE__URL` | PostgreSQL (migrations au démarrage) |
| `AEGIS__IDENTITY__SECRET` | Secret HMAC partagé avec la passerelle (`GATEWAY__IDENTITY__SECRET`) ; vide = refus de démarrer |
| `AEGIS__REDIS__URL`, `AEGIS__REDIS__PREFIX` | Cache et pub/sub (vide = sans cache ni invalidation, les applications retombent sur leur TTL) |
| `AEGIS__SERVICES__TOKENS` | `app:jeton,app2:jeton2` |
| `AEGIS__SERVER__ADMIN_ROLE` | Rôle Keycloak super-admin (défaut `aegis-admin`) |
| `AEGIS__GATEWAY__URL`, `AEGIS__GATEWAY__INTERNAL_TOKEN` | Annuaire : `/internal/users` de la passerelle (avec `GATEWAY__INTERNAL__LIST_ALL=true`) |
| `AEGIS__ANNUAIRE__SYNC_MINUTES` | Période de synchronisation Keycloak (15 ; 0 = désactivée) |
| `AEGIS__RESSOURCES__ALLOWED_HOSTS` | Hôtes autorisés pour les `list_url` (vide = tous) |
| `AEGIS__CACHE__TTL_SECS` | Durée maximale d'un droit en cache (600) |

Passerelle : route `/api/aegis` vers `http://aegis:8080` (`api/security/config/routes.toml`).

## Estompe

| Variable | Rôle |
| --- | --- |
| `AEGIS_URL` | URL interne d'aegis. Vide : Estompe reste sur l'ancien service permission (`PERMISSION_*`) |
| `AEGIS_TOKEN` | Jeton d'Estompe (`estompe:<jeton>` dans `AEGIS__SERVICES__TOKENS`) ; vérifie aussi les appels d'aegis vers `/internal/aegis/resources/*` |
| `AEGIS_REDIS_URL`, `AEGIS_REDIS_PREFIX` | Abonnement aux invalidations |
| `AEGIS_ADMIN_ROLE` | Rôle Keycloak qui ouvre Administration › accès sans compte Estompe (défaut `aegis-admin`) |
| `AEGIS_RESOURCES_URL` | URL par laquelle aegis joint Estompe (défaut `http://estompe-api:8080`) |

Estompe envoie son manifeste (permissions des blocs actifs), applique la garde du catalogue sur ses routes, plafonne les rôles
de serveur, renvoie `capabilities` dans `GET /me` (confort d'affichage) et relaie `/admin/acces/*` vers `/v1/admin/*` d'aegis
avec le justificatif de l'appelant : aegis reste seul juge. Détail : [fonctionnalites.md](fonctionnalites.md).

En mode permission (`AEGIS_URL` vide), les droits sont traduits : `estompe:access` (ou `PERMISSION_NAME`), `estompe:movies` →
`estompe:films.use`, `estompe:admin` → toutes les permissions `estompe:admin.*`, et les permissions ouvertes par défaut sont accordées.

## pli

`AEGIS_URL` et `AEGIS_TOKEN` (prioritaires sur `PERMISSION_URL` et `PERMISSION_SERVICE_TOKEN`) : pli envoie son manifeste
(`pli:deposit`, fermée par défaut) et vérifie le dépôt par `POST /v1/check`.

## Entracte

`AEGIS_URL` et `ENTRACTE_AEGIS_TOKEN` (jeton `entracte:<jeton>` de `AEGIS__SERVICES__TOKENS`) : Entracte envoie son manifeste
(`entracte:access`, `entracte:request`, `entracte:admin`, fermées par défaut) et lit les droits par `POST /v1/effective` (cache borné par
`valid_until`, 30 s au plus). Sans aegis, toutes ses permissions sont refusées. Voir [docs/entracte](../entracte/README.md).

## Bascule depuis permission

1. Déployer aegis (base, Redis, route de la passerelle), avec les jetons d'Estompe et de pli.
2. Donner le rôle Keycloak `aegis-admin` (et `user`) aux administrateurs.
3. Renseigner `AEGIS_URL`, `AEGIS_TOKEN` et `AEGIS_REDIS_URL` d'abord sur une instance de test d'Estompe pour recevoir le manifeste,
   puis configurer dans Administration › accès : un rôle `Membre` avec `estompe:access` (ou l'ajouter à « Tout le monde »),
   `estompe:films.use` aux bons comptes, les permissions `estompe:admin.*` aux administrateurs, `pli:deposit` aux déposants.
4. Basculer Estompe et pli en production en renseignant leurs variables `AEGIS_*` ; revenir en arrière = les vider.
5. Une fois la bascule confirmée : retirer `api/permission`, `web/permission`, `docs/permission`, le relais `/admin/permissions`
   d'Estompe, leurs jobs de CI, compose et tags.
