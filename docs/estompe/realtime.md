# Temps réel (WebSocket)

Les évènements arrivent par un WebSocket unique par client. Les envois (messages, réactions…) passent par l'API REST ; le
WebSocket ne sert qu'à recevoir, plus un ping applicatif.

## Connexion

1. `POST /ws-ticket` (authentifié comme le reste de l'API : cookie ou Bearer) → `{"ticket": "..."}`.
   Un navigateur ne peut pas mettre d'en-tête sur un WebSocket, d'où le ticket : à usage unique, valable 30 s.
2. Ouvrir `wss://<hôte>/ws?ticket=<ticket>` (facultatif : `&platform=web|pwa|windows|android&v=<version>`, affichés dans le tableau de bord d'administration et journalisés ; une version sous le minimum de la plateforme est refusée par 426 `upgrade_required`). L'en-tête `Origin`, s'il est présent, doit être autorisé.
3. Le service revalide la session auprès du gateway toutes les 2 minutes. Si elle a expiré, il ferme avec le code **4401** :
   le client doit se reconnecter (nouveau ticket, après avoir rafraîchi sa session).
   Il vérifie aussi la permission `estompe:access` : si elle est retirée (ou si le service de permission reste injoignable
   plus de 10 minutes), il expulse l'utilisateur du vocal et ferme avec le code **4403** : le client ne se reconnecte pas
   et affiche l'écran d'accès refusé.
4. Un client trop lent (64 évènements en attente) est déconnecté : reconnecter, puis recharger l'état utile
   (historique du salon ouvert, notifications) puisque des évènements ont pu être perdus.

Après chaque reconnexion, rechargez l'historique des salons visibles : le serveur ne rejoue pas les évènements manqués.

## Format

Chaque trame texte est un JSON `{"type": "...", "data": ...}`. Pour garder la connexion vivante côté client, envoyer
`{"type":"ping"}` ; le serveur répond `{"type":"pong"}`.

## Évènements

| Type | Destinataires | `data` |
| --- | --- | --- |
| `ready` | l'utilisateur | `{user_id}` — premier évènement |
| `presence.snapshot` | l'utilisateur | `{online: [user_id], activities: {user_id: jeu}}` — connectés parmi les personnes qui partagent un serveur ou une conversation avec lui, et le jeu en cours de ceux qui en partagent un |
| `avatar.updated` | personnes qui partagent un serveur ou une conversation (et soi-même) | `{user_id, version}` — `version` 0 = avatar supprimé ; l'image se charge par `GET /users/{id}/avatar` |
| `typing` | autres participants de la conversation | `{channel_id|dm_id, user_id, pseudo}` — à afficher environ 5 s |
| `read.updated` | participants de la conversation | `{channel_id|dm_id, user_id, pseudo, last_id, read_at}` — `user_id` a vu les messages jusqu'à `last_id` ; émis seulement quand `last_id` avance ; reçu pour son propre `user_id`, les autres appareils vident pastille et mentions de la conversation |
| `status.updated` | personnes qui partagent un serveur ou une conversation (et soi-même) | `{user_id, status}` (`online`, `idle`, `dnd`) |
| `activity.updated` | personnes qui partagent un serveur ou une conversation (et soi-même) | `{user_id, name}` — jeu en cours (`name` null : aucun) ; non stocké, effacé à la dernière déconnexion de l'utilisateur (le client le renvoie à chaque connexion) |
| `notify_settings.updated` | l'utilisateur (tous ses appareils) | réglages complets, comme `GET /me/notify-settings` : `{levels, types, preview, sound}` |
| `notification` | l'utilisateur, s'il a au moins une app connectée (sinon push Web/FCM) | notification déjà décidée par le serveur selon ses réglages : `{title, body, tag, url, conv?, msg?, kind, silent, sender?, room?, text?, attach?}` (`kind` : `dm`, `mention`, `channel`, `voice` ; `sender` auteur, `room` nom du salon (vide en message privé), `text` contenu (500 caractères au plus), `attach` `image` ou `file` : absents quand l'aperçu est masqué) ; le client l'affiche (web, toast Windows, Android) sauf si la conversation `conv` est sous les yeux de l'utilisateur, et joue le bip si `silent` est faux |
| `emoji.updated` | membres du serveur | `{server_id}` — recharger `GET /emojis` |
| `presence` | personnes qui partagent un serveur ou une conversation | `{user_id, online}` — premier appareil connecté / dernier déconnecté |
| `message.created` | participants du salon ou de la conversation | objet `Message` complet |
| `message.updated` | idem | objet `Message` complet (aussi quand la transcription d'un message vocal est calculée, corrigée ou retirée) |
| `message.deleted` | idem | `{id, channel_id, dm_id}` |
| `poll.voted` | idem | `{message_id, user_id, options, counts}` — `options` : choix actuels de `user_id` ; `counts` : votes par option |
| `poll.closed` | idem | `{message_id, closes_at}` — clôture manuelle (lanceur, propriétaire du serveur, admin ou modérateur « Gérer les messages ») ; la même carte affiche alors les résultats, sans second message |
| `reaction.updated` | idem | `{message_id, emoji, user_id, pseudo, added}` — le drapeau `me` se déduit de `user_id` |
| `mention` | l'utilisateur mentionné | objet `Message` (en plus de `message.created`/`updated` ; sert aux notifications), avec `group_mention: true` si la personne n'est mentionnée que par @everyone ou @here |
| `channel.created` | membres du serveur ; pour un salon privé, ceux qui le voient | `{server_id, channel, after}` — `after` : salon du même type après lequel il est placé (vide : à la fin) (EST-159) |
| `channel.updated` | idem | `{server_id, id, name}` |
| `channel.reordered` | membres du serveur | `{server_id, kind, ids}` — nouvel ordre des salons du type `kind` (EST-125) |
| `banner.updated` | la personne et ses contacts | `{user_id, color}` — couleur de bannière du profil (`#rrggbb`, vide : couleur du rôle le plus élevé) (EST-124) |
| `channel.deleted` | idem | `{server_id, id}` |
| `server.updated` | idem | `{id, name}` |
| `server_icon.updated` | membres du serveur | `{id, version}` — `version` 0 = icône retirée (initiales) ; l'image se charge par `GET /servers/{id}/icon` |
| `server.deleted` | membres, juste avant la suppression | `{id}` |
| `member.joined` | membres (dont le nouvel arrivant) | `{server_id, member: {id, pseudo, online}}` — `online` évite d'attendre un évènement `presence` pour un arrivant déjà connecté |
| `member.left` | membres restants et la personne partie | `{server_id, user_id, kind?, reason?, until?}` — `kind` vaut `kick` ou `ban` quand la personne a été expulsée ou bannie (`reason` : raison donnée, `until` : fin du ban, nul si définitif) ; absent pour un départ volontaire (EST-106) |
| `member.timeout` | membres du serveur | `{server_id, user_id, until, reason}` — exclusion temporaire posée (`until` : fin) ou levée (`until` nul). Le client de la personne recharge `GET /servers/{id}` (permissions réduites) et recharge à l'échéance |
| `member.warned` | le membre averti | `{server_id, reason}` |
| `member.nickname` | membres du serveur | `{server_id, user_id, nickname}` — pseudo de serveur (vide : retiré) |
| `messages.deleted` | membres qui voient le salon | `{channel_id, ids}` — suppression en masse (modération, purge d'un ban) |
| `voice.moved` | le membre déplacé | `{server_id, from, to, by, to_name}` — le serveur l'a retiré du salon `from` ; son client rejoint `to` aussitôt (la salle `from` n'est quittée qu'une fois `to` connectée, même si le retrait précède l'évènement), en gardant son état micro et son ; `by` = pseudo du modérateur, `to_name` = nom du salon cible |
| `voice.disconnected` | le membre déconnecté | `{server_id, channel_id}` |
| `server_avatar.updated` | membres du serveur | `{server_id, user_id, version}` — `version` 0 = photo du serveur retirée (retour à la photo globale) ; l'image se charge par `GET /servers/{id}/users/{uid}/avatar` |
| `roles.updated` | membres du serveur | `{server_id, user_id?}` — rôles, permissions ou exceptions de salon modifiés (`user_id` : un seul membre concerné). Le client recharge `GET /servers/{id}` ; en vocal, le serveur ajuste ou coupe aussitôt les droits des présents (EST-104) |
| `voice.state` | membres qui voient le salon | `{server_id, channel_id, user_id, pseudo, joined, at}` — arrivée ou départ dans un salon vocal (via les webhooks LiveKit) ; `at` sert de point de départ à la durée de présence, et un départ met à jour `GET /servers/{id}/voice/stats` |
| `voice.mod` | membres qui voient le salon | `{server_id, channel_id, user_id, mic_off, sound_off}` — un modérateur (« Couper le micro / le son des membres ») coupe ou rétablit le micro et/ou le son d'un participant (EST-102) ; l'état est aussi porté par `GET /servers/{id}/voice` |
| `watch.state` | membres qui voient le salon | `{server_id, channel_id, state, now}` — session « Regarder ensemble » du salon vocal (EST-143), état complet à chaque changement (`state` nul : session arrêtée) : `{rev, provider, video, playing, pos, rate, at, host, host_only, by_id, by, act, queue[{id, provider, video, by, by_id, meta?}], ready{user_id: bool}, meta?, repeat, history[…]}` (`repeat` : "", `one`, `all` ; `act` vaut aussi `repeat` quand le même morceau est relu depuis le début ; `meta` n'est présent que pour Music2gether, voir `music.state`) (`provider` : youtube, twitch_vod, twitch_clip, twitch_live, vimeo, dailymotion, file ; `video` : identifiant propre à la source, ou lien du fichier). Position courante = `pos` + (heure − `at`) × `rate` si `playing` ; `now` est l'heure du serveur à l'émission (décalage d'horloge). Un client rejoint en cours de route avec `GET /voice/{salon}/watch`. Les évènements peuvent arriver dans le désordre : le client ignore un état dont `rev` est inférieure à celui déjà appliqué (même `rev` : même état de lecture, seul `ready` diffère) |
| `cinema.state` | membres qui voient le salon | `{server_id, channel_id, state, now}` — séance du salon cinéma (bloc `regarder-ensemble`), même forme et mêmes règles d'ordre (`rev`) que `watch.state`, avec toujours `state.provider = "film"` : `state.video` est l'identifiant public d'un titre d'Entracte, `state.host_only` vaut toujours `true` (l'hôte pilote seul), `state.queue` porte les épisodes suivants et `state.meta` le titre et la durée. Chaque participant ouvre son propre flux par `POST /titles/{id}/playback` de l'API d'Entracte (qualité, piste audio et sous-titres à lui, seule la position est synchronisée) ; un participant sans `entracte:access` reçoit l'état mais Entracte lui refuse le flux (403). Un client rejoint en cours de route avec `GET /voice/{salon}/cinema`. « Regarder ensemble » (`watch.state`) ne lit plus de films |
| `music.state` | membres qui voient le salon | `{server_id, channel_id, state, now}` — session Music2gether du salon vocal (EST-168), distincte de la session « Regarder ensemble » du même salon (`watch.state`) ; même forme d'état, avec `meta` `{title, artist, album, duration, cover}` (le morceau YouTube Music en cours ; aussi sur chaque élément de la file) ; `provider` vaut toujours `youtube`. `state` nul : session arrêtée. Un client rejoint en cours de route avec `GET /voice/{salon}/music` ; mêmes règles d'ordre (`rev`) que `watch.state` |
| `draw.state` | membres qui voient le salon | `{server_id, channel_id, state, now}` — partie Draw2Gether du salon vocal (EST-182), état complet à chaque changement de phase, de joueur ou de points, et à chaque indice (`state` nul : partie arrêtée) ; forme de `state` : voir `GET /voice/{salon}/draw`. Les chronos sont tenus par le serveur : `deadline` et `now` sont en millisecondes Unix, le client ne déclenche aucune transition. Le mot n'y figure jamais avant la fin du tour (`reveal`). Même règle d'ordre que `watch.state` : le client ignore un état dont `rev` est inférieure à celle déjà appliquée ; un client rejoint en cours de route avec `GET /voice/{salon}/draw` |
| `draw.private` | le dessinateur seul | `{server_id, channel_id, rev, choices?, word?}` — `choices` (mots proposés) pendant la phase `choosing`, `word` pendant la phase `drawing` |
| `draw.chat` | selon `kind` | `{server_id, channel_id, kind, from, pseudo, text, at}` — chat de la partie. `guess` (proposition fausse) et `chat` (hors tour) : membres qui voient le salon ; `found` (« X a trouvé ») : membres qui voient le salon, `text` vide ; `close` (mot proche) : le joueur seul, `text` vide ; `chat_private` : le dessinateur et les joueurs qui ont trouvé. `at` en millisecondes Unix |
| `draw.ops` | membres qui voient le salon | `{server_id, channel_id, seq, ops[]}` — opérations de dessin vectorielles `{op: stroke, id, tool, color, size, pts}`, `{op: undo}` ou `{op: clear}`, numérotées par `seq` (un par opération, `state.draw_seq` à jour). Les évènements peuvent arriver dans le désordre : le client applique par `seq` croissant et, s'il manque un numéro, relit `drawing` avec `GET /voice/{salon}/draw` |
| `videos.changed` | membres qui voient le salon | `{server_id, channel_id}` — le contenu d'un salon « Vidéos » ou « Musique » a changé (carte publiée ou enrichie, vue par le groupe, liste, séance planifiée, répondue, annulée ou lancée, EST-149) ; le client qui l'a ouvert recharge `GET /channels/{salon}/videos`. Réactions, réponses et suppressions de messages passent par leurs évènements habituels (le compte de la carte suit `reaction.updated`) |
| `pong` | l'utilisateur | réponse à `ping` |

**Réactions et `me`** : dans `message.created`/`message.updated`, le champ `reactions[].me` est calculé pour l'auteur du message, pas pour le destinataire. Le client garde ses propres réactions (mises à jour par `reaction.updated`) et ne reprend du message reçu que le contenu, les pièces jointes, les mentions et `edited_at`.

Une nouvelle conversation privée n'a pas d'évènement dédié : le premier `message.created` porte un `dm_id` inconnu du
client, qui recharge alors `GET /dms`.

## Vocal

`POST /voice/{salon}/token` renvoie `{url, token, room, max_participants}` à passer au client LiveKit
(`livekit-client`). Le salon est limité à 10 participants (409 `voice_full` sinon). En v0 le jeton n'autorise que le
micro ; la caméra et le partage d'écran (v1) sont autorisés par `LIVEKIT_ALLOW_VIDEO=true`, valeur par défaut du déploiement, sans changer l'API.
Spectateurs : chaque client publie dans ses attributs LiveKit `watching` la liste (séparée par des virgules) des `sid` de pistes vidéo qu'il affiche ; les autres participants de la salle en déduisent les spectateurs. Rien côté serveur : le départ ou la déconnexion efface l'attribut avec le participant.

`GET /servers/{id}/voice` donne les participants de chaque salon vocal ; ensuite `voice.state` tient la liste à jour.
