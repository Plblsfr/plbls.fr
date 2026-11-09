# Fonctionnalités protégées

Chaque fonctionnalité d'Estompe a sa permission, déclarée par le code (`api/estompe/core/src/catalogue.rs`) et envoyée à aegis
par manifeste. Trois mécanismes l'appliquent :

- **Garde des routes** : chaque route listée vérifie la permission (403 `forbidden` sinon, 503 si aegis est injoignable).
  Un test vérifie que chaque motif correspond à une route documentée dans l'OpenAPI.
- **Plafond des rôles de serveur** : une permission « plafond » refusée sur un serveur retire le droit de serveur correspondant,
  propriétaire compris. Un serveur ne peut jamais donner ce qu'aegis refuse ; les droits de modération restent propres au serveur.
- **Interface** : `GET /me` renvoie `capabilities` ; le front masque ce qui est refusé (confort d'affichage seulement).

« Défaut » : ouverte = accordée à « Tout le monde » à sa première déclaration ; fermée = à accorder explicitement.
Une permission dont le bloc est désactivé n'est pas déclarée.

## Estompe

| Catégorie | Permission | Libellé | Défaut | Ressource | Bloc | Routes protégées | Plafond |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Accès | `estompe:access` | Utiliser Estompe | fermée | — | — | — | — |
| Serveurs | `estompe:servers.create` | Créer un serveur | ouverte | — | — | `POST /servers` | — |
| Serveurs | `estompe:servers.join` | Rejoindre un serveur | ouverte | — | — | `POST /server-invites/{code}/join` | — |
| Serveurs | `estompe:servers.invite` | Inviter | ouverte | serveur | — | — | create_invites |
| Serveurs | `estompe:servers.icon` | Icône de serveur | ouverte | — | — | `PUT /servers/{id}/icon`<br>`DELETE /servers/{id}/icon` | — |
| Messages | `estompe:messages.send` | Écrire dans les salons | ouverte | serveur | — | — | send_messages |
| Messages | `estompe:dms.use` | Messages privés | ouverte | — | — | `POST /dms`<br>`POST /dms/{id}/messages` | — |
| Messages | `estompe:files.upload` | Joindre des fichiers | ouverte | serveur | — | `POST /files` | attach_files |
| Messages | `estompe:reactions.add` | Réagir | ouverte | serveur | — | `PUT /messages/{id}/reactions/{emoji}` | add_reactions |
| Messages | `estompe:polls.create` | Créer des sondages | ouverte | serveur | — | — | create_polls |
| Messages | `estompe:polls.vote` | Voter | ouverte | — | — | `PUT /messages/{id}/vote` | — |
| Messages | `estompe:mention.everyone` | Mentionner tout le monde | ouverte | serveur | — | — | mention_everyone |
| Messages | `estompe:search.use` | Rechercher | ouverte | — | — | `GET /search`<br>`GET /channels/{cid}/search`<br>`GET /dms/{id}/search` | — |
| Messages | `estompe:gifs.use` | GIF | ouverte | — | gifs | `POST /gifs/import`<br>`GET /gifs/search`<br>`GET /gifs/trending` | — |
| Messages | `estompe:link-preview.use` | Aperçus de liens | ouverte | — | apercu-liens | `GET /link-preview` | — |
| Messages | `estompe:emojis.manage` | Gérer les emojis | ouverte | — | emojis | `POST /servers/{id}/emojis`<br>`PATCH /servers/{id}/emojis/{eid}`<br>`DELETE /servers/{id}/emojis/{eid}` | — |
| Vocal | `estompe:vocal.join` | Rejoindre le vocal | ouverte | serveur | vocal | `POST /voice/{cid}/token` | connect + speak |
| Vocal | `estompe:vocal.video` | Caméra | ouverte | serveur | vocal | — | video |
| Vocal | `estompe:vocal.screen` | Partage d'écran | ouverte | serveur | vocal | — | share_screen |
| Vocal | `estompe:music.use` | Musique | ouverte | — | musique | `GET /music/search`<br>`GET /music/resolve`<br>`PUT /voice/{cid}/music/**`<br>`POST /voice/{cid}/music/**`<br>`DELETE /voice/{cid}/music/**` | — |
| Vocal | `estompe:watch.organize` | Regarder ensemble | ouverte | serveur | regarder-ensemble | `PUT /voice/{cid}/watch/**`<br>`POST /voice/{cid}/watch/**`<br>`DELETE /voice/{cid}/watch/**` | organize_watch |
| Vocal | `estompe:draw2gether.use` | Draw2Gether | ouverte | — | draw2gether | `PUT /voice/{cid}/draw/**`<br>`POST /voice/{cid}/draw/**`<br>`DELETE /voice/{cid}/draw/**` | — |
| Vocal | `estompe:transcription.use` | Transcription | ouverte | — | transcription | `POST /files/{id}/transcript`<br>`PUT /files/{id}/transcript` | — |
| Médias | `estompe:films.use` | Films et séries | fermée | — | films | `* /films/**`<br>`* /voice/{cid}/cinema/**`<br>`GET /servers/{id}/cinema` | — |
| Médias | `estompe:videos.use` | Salons Vidéos | ouverte | — | videos | `POST /channels/{cid}/videos/**`<br>`PUT /channels/{cid}/videos/**`<br>`DELETE /channels/{cid}/videos/**` | — |
| Profil | `estompe:profile.avatar` | Avatar | ouverte | — | avatars | `PUT /me/avatar`<br>`DELETE /me/avatar`<br>`PUT /servers/{id}/me/avatar`<br>`DELETE /servers/{id}/me/avatar` | — |
| Profil | `estompe:profile.status` | Statut et activité | ouverte | — | activite-statut | `PUT /me/status`<br>`PUT /me/activity` | — |
| Profil | `estompe:profile.banner` | Bannière | ouverte | — | activite-statut | `PUT /me/banner` | — |
| Profil | `estompe:profile.camera-background` | Fond de caméra | ouverte | — | — | `PUT /me/camera-background` | — |
| Profil | `estompe:notifications.push` | Notifications push | ouverte | — | notifications | `PUT /push/subscription`<br>`PUT /push/fcm`<br>`PUT /push/desktop`<br>`POST /push/test` | — |
| Profil | `estompe:reports.create` | Signaler un bug | ouverte | — | signalements | `POST /reports` | — |
| Administration | `estompe:admin.view` | Vue d'ensemble | fermée | — | — | `GET /admin/overview`<br>`GET /admin/stats`<br>`GET /admin/live` | — |
| Administration | `estompe:admin.users` | Comptes Estompe | fermée | — | — | `* /admin/users/**` | — |
| Administration | `estompe:admin.servers` | Serveurs | fermée | — | — | `* /admin/servers/**` | — |
| Administration | `estompe:admin.logs` | Journaux | fermée | — | — | `GET /admin/log`<br>`GET /admin/errors` | — |
| Administration | `estompe:admin.bot` | Compte « Estompe » | fermée | — | — | `* /admin/bot/**` | — |
| Administration | `estompe:admin.badges` | Badges | fermée | — | badges | `* /admin/badges/**` | — |
| Administration | `estompe:admin.reports` | Signalements | fermée | — | signalements | `* /admin/reports/**` | — |
| Administration | `estompe:admin.films` | Fichiers écartés | fermée | — | films | `GET /admin/films/rejets` | — |
| Administration | `estompe:admin.files` | Fichiers signalés | fermée | — | — | — | — |
| Administration | `estompe:admin.polls` | Sondages | fermée | — | — | — | — |

`estompe:access` est vérifiée à chaque appel authentifié (compte activé) et par le WebSocket ; `estompe:admin.files` ouvre
les fichiers joints aux signalements, `estompe:admin.polls` permet de clore n'importe quel sondage. Avoir au moins une
permission `estompe:admin.*` ouvre l'espace d'administration, dont les sections suivent chacune leur permission.

## pli

| Permission | Libellé | Défaut | Contrôle |
| --- | --- | --- | --- |
| `pli:deposit` | Déposer des fichiers | fermée | `/api/me`, `/api/shares` et la gestion des dépôts (via `POST /v1/check`) |

## aegis

| Permission | Libellé | Défaut |
| --- | --- | --- |
| `aegis:admin` | Administrer les permissions | fermée (rôle système « Administrateur aegis ») |
