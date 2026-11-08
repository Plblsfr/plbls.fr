# Intégration

## Depuis un micro-service (réseau interne)

Deux façons de s'authentifier auprès de permission :

1. **Pour l'utilisateur courant** : faire suivre tels quels les en-têtes `X-User-*` / `X-Identity-*` reçus du gateway. Permission en vérifie la signature (HMAC, 60 s) avec le même secret que le gateway. L'appelant ne peut lire que **ses propres** permissions.
2. **Pour n'importe quel compte** : `Authorization: Bearer <token du service>`. Les tokens sont déclarés côté permission : `PERMISSION__SERVICES__TOKENS=chat:<token>,radar:<token>` (un par service, révocable séparément). Un token de service ne peut **que lire** (check, permissions d'un compte), jamais administrer.

### Vérifier une action

```http
POST /v1/check
Authorization: Bearer <token du service chat>

{"user_id": "<sub>", "permission": "chat:message.delete"}
→ {"allowed": true}
```

### Charger toutes les permissions d'un compte (pour mettre en cache)

```http
GET /v1/users/<sub>/permissions?app=chat
→ {"user_id": "<sub>", "permissions": ["chat:message.delete", "chat:user.mute"]}
```

Conseils : cache de quelques dizaines de secondes par `sub` côté service (une révocation prend donc au plus ce délai) ; en cas d'erreur réseau, **refuser** plutôt qu'autoriser.

## Depuis un frontend

Via le gateway (`https://security.plbls.fr/api/permission/...`, cookie de session) : `GET /v1/me/permissions?app=chat` renvoie les permissions du connecté, pour afficher ou masquer des boutons. Ce n'est **qu'un confort d'affichage** : le contrôle réel se fait toujours côté service.

## Route gateway

Dans `config/routes.toml` de security.plbls.fr :

```toml
[[routes]]
prefix = "/api/permission"
upstream = "http://permission:8080"
strip_prefix = true
roles = ["user"]
```

L'administration est protégée **dans permission** par le rôle Keycloak `permission-admin` (réglable : `server.admin_role`) ou par la permission `permission:admin` (rôle `permission/admin`, attribuable depuis l'interface). Créer ce rôle dans le realm et l'attribuer aux administrateurs. Permission doit partager `GATEWAY__IDENTITY__SECRET` (`PERMISSION__IDENTITY__SECRET`) et être joignable depuis le gateway sur le réseau Docker.

## Déclarer une nouvelle application

1. `POST /v1/apps` `{"key":"chat","name":"Chat"}`
2. `POST /v1/apps/chat/permissions` pour chaque action testée par le service
3. `POST /v1/apps/chat/roles` avec la liste des permissions
4. `POST /v1/assignments` `{"user_id":"<sub>","app":"chat","role":"moderator"}`

(ou tout faire depuis l'interface : Administration › permissions d'Estompe, ou l'ancien front `permission.plbls.fr`).

## Interface d'administration dans Estompe

Le front unique sert l'administration de permission dans Estompe (section « permissions » de l'administration). L'API Estompe relaie
`/admin/permissions/*` vers `<gateway>/api/permission/v1/*` avec le justificatif de l'appelant : permission reste seul juge (rôle Keycloak
`permission-admin` ou `permission:admin`). Un compte qui n'a que le rôle Keycloak `permission-admin` se connecte à Estompe et n'y voit que
cette page, sans compte Estompe à activer (`PERMISSION_ADMIN_ROLE` côté Estompe, même valeur que `server.admin_role`). Ce compte doit
aussi porter le rôle demandé par la route `/api/permission` du gateway (`user`).

## MCP (assistants IA)

`POST /mcp` avec `Authorization: Bearer <PERMISSION__MCP__TOKEN>` (désactivé si vide). Outils : `list_docs`, `get_doc`, `get_openapi`, `list_operations`.

```sh
claude mcp add --transport http permission https://permission.plbls.fr/mcp --header "Authorization: Bearer <token>"
```
