# Fonctionnalités protégées

Fichier généré depuis `api/estompe/core/src/catalogue.rs` : ne pas le modifier à la main. Après un changement du catalogue,
`AEGIS_DOC=1 cargo test -p estompe-core catalogue::tests::documentation_a_jour` le régénère ; sans cette variable, le test échoue
tant que ce fichier ne correspond pas au code.

Chaque fonctionnalité d'Estompe a sa permission, envoyée à aegis par manifeste. Des tests déterministes l'imposent :

- **chaque route** documentée dans l'OpenAPI relève d'exactement une permission ou d'une exception motivée (liste en fin de fichier) ;
une nouvelle route sans décision fait échouer `estompe-api` ;
- **chaque droit de serveur** est plafonné par une permission, sauf ceux listés en fin de fichier ;
- **chaque permission** protège au moins une route, plafonne un droit de serveur, ou est contrôlée dans le code (liste) ;
- les permissions utilisées par le front (`app.peut`, sections d'administration) existent dans le catalogue.

Mécanismes : la **garde** vérifie la permission sur les routes listées (403 `forbidden`, 503 si aegis est injoignable ; pour une
permission par serveur, il suffit qu'elle soit accordée sur au moins un serveur) ; le **plafond** retire le droit de serveur
correspondant quand la permission est refusée sur ce serveur, propriétaire compris ; l'interface masque ce qui est refusé
(`capabilities` de `GET /me`).

« Défaut » : ouverte = accordée à « Tout le monde » à sa première déclaration ; fermée = à accorder explicitement.
Une permission dont le bloc est désactivé n'est pas déclarée.

## Accès

| Permission | Libellé | Défaut | Ressource | Bloc | Routes protégées | Plafond |
| --- | --- | --- | --- | --- | --- | --- |
| `estompe:access` | Utiliser Estompe | fermée | — | — | `GET /servers`<br>`GET /servers/{id}`<br>`POST /servers/{id}/leave`<br>`GET /servers/{id}/icon`<br>`GET /channels/{cid}/messages`<br>`GET /channels/{cid}/pins`<br>`POST /channels/{cid}/read`<br>`GET /channels/{cid}/reads`<br>`POST /channels/{cid}/typing`<br>`GET /dms`<br>`GET /dms/{id}/messages`<br>`GET /dms/{id}/pins`<br>`POST /dms/{id}/read`<br>`GET /dms/{id}/reads`<br>`POST /dms/{id}/typing`<br>`PATCH /messages/{id}`<br>`DELETE /messages/{id}`<br>`PUT /messages/{id}/pin`<br>`DELETE /messages/{id}/pin`<br>`DELETE /messages/{id}/reactions/{emoji}`<br>`GET /messages/{id}/poll/votes`<br>`GET /files/{id}`<br>`POST /files/{id}/link`<br>`GET /files/{id}/thumb`<br>`GET /emojis`<br>`GET /emojis/{eid}/image`<br>`GET /unread`<br>`GET /notifications`<br>`POST /notifications/read`<br>`GET /me/notify-settings`<br>`PUT /me/notify-settings`<br>`GET /me/camera-background`<br>`DELETE /me/camera-background`<br>`GET /avatars`<br>`GET /users/{id}/avatar`<br>`GET /servers/{id}/avatars`<br>`GET /servers/{id}/users/{uid}/avatar`<br>`GET /statuses`<br>`GET /users/{id}/badges`<br>`GET /badges/{id}/icon`<br>`GET /reports`<br>`GET /push/key`<br>`DELETE /push/subscription`<br>`DELETE /push/fcm`<br>`DELETE /push/desktop`<br>`GET /servers/{id}/voice`<br>`GET /servers/{id}/voice/stats`<br>`GET /servers/{id}/music`<br>`GET /servers/{id}/watch`<br>`GET /voice/{cid}/music`<br>`GET /voice/{cid}/watch`<br>`GET /voice/{cid}/draw`<br>`GET /channels/{cid}/videos`<br>`GET /channels/{cid}/videos/posts/{mid}/thumb`<br>`GET /ws`<br>`POST /ws-ticket` | — |

## Serveurs

| Permission | Libellé | Défaut | Ressource | Bloc | Routes protégées | Plafond |
| --- | --- | --- | --- | --- | --- | --- |
| `estompe:servers.create` | Créer un serveur | ouverte | — | — | `POST /servers` | — |
| `estompe:servers.join` | Rejoindre un serveur | ouverte | — | — | `POST /server-invites/{code}/join` | — |
| `estompe:servers.manage` | Gérer un serveur | ouverte | serveur | — | `PATCH /servers/{id}`<br>`DELETE /servers/{id}`<br>`PUT /servers/{id}/icon`<br>`DELETE /servers/{id}/icon`<br>`POST /servers/{id}/transfer` | manage_server |
| `estompe:servers.invite` | Inviter | ouverte | serveur | — | `GET /servers/{id}/invites`<br>`POST /servers/{id}/invites`<br>`DELETE /servers/{id}/invites/{code}` | create_invites |
| `estompe:channels.manage` | Gérer les salons | ouverte | serveur | — | `POST /servers/{id}/channels`<br>`PUT /servers/{id}/channels/order`<br>`PATCH /servers/{id}/channels/{cid}`<br>`DELETE /servers/{id}/channels/{cid}`<br>`PUT /servers/{id}/channels/{cid}/moderation`<br>`GET /servers/{id}/channels/{cid}/overrides`<br>`PUT /servers/{id}/channels/{cid}/overrides/{rid}` | manage_channels |
| `estompe:roles.manage` | Gérer les rôles | ouverte | serveur | — | `POST /servers/{id}/roles`<br>`PATCH /servers/{id}/roles/{rid}`<br>`DELETE /servers/{id}/roles/{rid}`<br>`PUT /servers/{id}/roles-order`<br>`PUT /servers/{id}/members/{uid}/roles/{rid}`<br>`DELETE /servers/{id}/members/{uid}/roles/{rid}` | manage_roles |
| `estompe:audit.view` | Journaux de serveur | ouverte | serveur | — | `GET /servers/{id}/audit-log`<br>`GET /servers/{id}/members/{uid}/history`<br>`GET /servers/{id}/voice/moderation-log` | view_audit_log |

## Modération

| Permission | Libellé | Défaut | Ressource | Bloc | Routes protégées | Plafond |
| --- | --- | --- | --- | --- | --- | --- |
| `estompe:members.kick` | Expulser | ouverte | serveur | — | `POST /servers/{id}/members/{uid}/kick` | kick_members |
| `estompe:members.ban` | Bannir | ouverte | serveur | — | `POST /servers/{id}/members/{uid}/ban`<br>`GET /servers/{id}/bans`<br>`DELETE /servers/{id}/bans/{uid}` | ban_members |
| `estompe:members.timeout` | Exclure temporairement | ouverte | serveur | — | `PUT /servers/{id}/members/{uid}/timeout`<br>`DELETE /servers/{id}/members/{uid}/timeout`<br>`POST /servers/{id}/members/{uid}/warn` | timeout_members |
| `estompe:members.nickname` | Pseudos de serveur | ouverte | serveur | — | `PUT /servers/{id}/members/{uid}/nickname` | manage_nicknames |
| `estompe:messages.manage` | Gérer les messages | ouverte | serveur | — | `POST /channels/{cid}/messages/bulk-delete` | manage_messages |
| `estompe:vocal.moderate` | Modérer le vocal | ouverte | serveur | vocal | `PUT /voice/{cid}/participants/{uid}/mod`<br>`PUT /voice/{cid}/participants/{uid}/move`<br>`POST /voice/{cid}/participants/{uid}/disconnect` | mute_members + deafen_members + move_members + disconnect_members |

## Messages

| Permission | Libellé | Défaut | Ressource | Bloc | Routes protégées | Plafond |
| --- | --- | --- | --- | --- | --- | --- |
| `estompe:messages.send` | Écrire dans les salons | ouverte | serveur | — | `POST /channels/{cid}/messages` | send_messages |
| `estompe:dms.use` | Messages privés | ouverte | — | — | `POST /dms`<br>`POST /dms/{id}/messages` | — |
| `estompe:files.upload` | Joindre des fichiers | ouverte | serveur | — | `POST /files` | attach_files |
| `estompe:reactions.add` | Réagir | ouverte | serveur | — | `PUT /messages/{id}/reactions/{emoji}` | add_reactions |
| `estompe:polls.create` | Créer des sondages | ouverte | serveur | — | `POST /messages/{id}/poll/close` | create_polls |
| `estompe:polls.vote` | Voter | ouverte | — | — | `PUT /messages/{id}/vote`<br>`DELETE /messages/{id}/vote` | — |
| `estompe:mention.everyone` | Mentionner tout le monde | ouverte | serveur | — | — | mention_everyone |
| `estompe:search.use` | Rechercher | ouverte | — | — | `GET /search`<br>`GET /channels/{cid}/search`<br>`GET /dms/{id}/search` | — |
| `estompe:gifs.use` | GIF | ouverte | — | gifs | `POST /gifs/import`<br>`GET /gifs/search`<br>`GET /gifs/trending` | — |
| `estompe:link-preview.use` | Aperçus de liens | ouverte | — | apercu-liens | `GET /link-preview` | — |
| `estompe:emojis.manage` | Gérer les emojis | ouverte | — | emojis | `POST /servers/{id}/emojis`<br>`PATCH /servers/{id}/emojis/{eid}`<br>`DELETE /servers/{id}/emojis/{eid}` | — |

## Vocal

| Permission | Libellé | Défaut | Ressource | Bloc | Routes protégées | Plafond |
| --- | --- | --- | --- | --- | --- | --- |
| `estompe:vocal.join` | Rejoindre le vocal | ouverte | serveur | vocal | `POST /voice/{cid}/token` | connect + speak |
| `estompe:vocal.video` | Caméra | ouverte | serveur | vocal | — | video |
| `estompe:vocal.screen` | Partage d'écran | ouverte | serveur | vocal | — | share_screen |
| `estompe:music.use` | Musique | ouverte | — | musique | `GET /music/search`<br>`GET /music/resolve`<br>`GET /music/cover/{id}`<br>`PUT /voice/{cid}/music/**`<br>`POST /voice/{cid}/music/**`<br>`DELETE /voice/{cid}/music/**` | — |
| `estompe:watch.organize` | Regarder ensemble | ouverte | serveur | regarder-ensemble | `PUT /voice/{cid}/watch/**`<br>`POST /voice/{cid}/watch/**`<br>`DELETE /voice/{cid}/watch/**` | organize_watch |
| `estompe:draw2gether.use` | Draw2Gether | ouverte | — | draw2gether | `PUT /voice/{cid}/draw/**`<br>`POST /voice/{cid}/draw/**`<br>`DELETE /voice/{cid}/draw/**` | — |
| `estompe:transcription.use` | Transcription | ouverte | — | transcription | `POST /files/{id}/transcript`<br>`PUT /files/{id}/transcript` | — |

## Médias

| Permission | Libellé | Défaut | Ressource | Bloc | Routes protégées | Plafond |
| --- | --- | --- | --- | --- | --- | --- |
| `estompe:films.use` | Films et séries | fermée | — | films | `* /films/**`<br>`* /voice/{cid}/cinema/**`<br>`GET /servers/{id}/cinema` | — |
| `estompe:videos.use` | Salons Vidéos | ouverte | — | videos | `POST /channels/{cid}/videos/**`<br>`PUT /channels/{cid}/videos/**`<br>`DELETE /channels/{cid}/videos/**` | — |

## Profil

| Permission | Libellé | Défaut | Ressource | Bloc | Routes protégées | Plafond |
| --- | --- | --- | --- | --- | --- | --- |
| `estompe:profile.avatar` | Avatar | ouverte | — | avatars | `PUT /me/avatar`<br>`DELETE /me/avatar`<br>`PUT /servers/{id}/me/avatar`<br>`DELETE /servers/{id}/me/avatar` | — |
| `estompe:profile.status` | Statut et activité | ouverte | — | activite-statut | `PUT /me/status`<br>`PUT /me/activity` | — |
| `estompe:profile.banner` | Bannière | ouverte | — | activite-statut | `PUT /me/banner` | — |
| `estompe:profile.camera-background` | Fond de caméra | ouverte | — | — | `PUT /me/camera-background` | — |
| `estompe:notifications.push` | Notifications push | ouverte | — | notifications | `PUT /push/subscription`<br>`PUT /push/fcm`<br>`PUT /push/desktop`<br>`POST /push/test` | — |
| `estompe:reports.create` | Signaler un bug | ouverte | — | signalements | `POST /reports` | — |

## Administration

| Permission | Libellé | Défaut | Ressource | Bloc | Routes protégées | Plafond |
| --- | --- | --- | --- | --- | --- | --- |
| `estompe:admin.view` | Vue d'ensemble | fermée | — | — | `GET /admin/overview`<br>`GET /admin/stats`<br>`GET /admin/live` | — |
| `estompe:admin.users` | Comptes Estompe | fermée | — | — | `* /admin/users/**` | — |
| `estompe:admin.servers` | Serveurs | fermée | — | — | `* /admin/servers/**` | — |
| `estompe:admin.logs` | Journaux | fermée | — | — | `GET /admin/log`<br>`GET /admin/errors` | — |
| `estompe:admin.bot` | Compte « Estompe » | fermée | — | — | `* /admin/bot/**` | — |
| `estompe:admin.badges` | Badges | fermée | — | badges | `* /admin/badges/**` | — |
| `estompe:admin.reports` | Signalements | fermée | — | signalements | `* /admin/reports/**` | — |
| `estompe:admin.films` | Fichiers écartés | fermée | — | films | `GET /admin/films/rejets` | — |
| `estompe:admin.files` | Fichiers signalés | fermée | — | — | contrôle dans le code : fichiers::acces : ouvre un fichier hors conversation quand il est joint à un signalement | — |
| `estompe:admin.polls` | Sondages | fermée | — | — | contrôle dans le code : sondages::peut_gerer : clore le sondage d'un autre | — |

## Routes hors permission

| Route | Motif |
| --- | --- |
| `GET /healthz` | supervision : sonde de vie |
| `GET /readyz` | supervision : sonde de disponibilité |
| `GET /metrics` | supervision : jeton METRICS_TOKEN |
| `GET /me` | état du compte : répond aussi à qui n'a pas estompe:access |
| `POST /activate` | activation du compte : vérifie estompe:access elle-même |
| `POST /voice/webhook` | webhook LiveKit signé, sans compte |
| `GET /internal/aegis/resources/{type}` | route interne appelée par aegis avec son jeton |
| `* /admin/acces/**` | administration des accès : jugée par aegis (aegis:admin ou rôle Keycloak) |
| `* /admin/permissions/**` | ancien service permission : jugé par permission, retiré après la bascule |

## Droits de serveur non plafonnés

| Droit | Motif |
| --- | --- |
| `view_channel` | visibilité des salons, réglée par chaque serveur |

## pli et aegis

| Permission | Libellé | Défaut | Contrôle |
| --- | --- | --- | --- |
| `pli:deposit` | Déposer des fichiers | fermée | `/api/me`, `/api/shares` et la gestion des dépôts (via `POST /v1/check`) |
| `aegis:admin` | Administrer les permissions | fermée | toute l'API d'administration ; rôle système « Administrateur aegis » |
