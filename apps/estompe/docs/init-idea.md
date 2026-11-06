# Cahier des charges — Appli de chat

Oct 12, 2026 · @Plbls

## Contexte et objectifs

L'appli est une messagerie écrite et vocale pour un petit groupe d'amis, sur le modèle de Discord et TeamSpeak. Elle doit rester légère : peu de processeur, peu de RAM, démarrage rapide.

- Messages privés, serveurs, salons texte et vocal dès la v0
- Un seul code et un seul design pour le web, Windows et mobile
- Performance mesurée avec des cibles chiffrées (section 8)

## Périmètre v0 et v1

La v0 couvre la messagerie et le vocal. La v1 ajoute uniquement la caméra et le partage d'écran.

| Fonction | v0 | v1 |
| --- | --- | --- |
| Messages privés | Oui | Inchangé |
| Serveurs et salons | Oui | Inchangé |
| Salons texte | Oui | Inchangé |
| Salons vocaux | Oui | Inchangé |
| Chat formaté, images, fichiers | Oui | Inchangé |
| Réactions, mentions, modifier et supprimer | Oui | Inchangé |
| Caméra | Non | Oui |
| Partage d'écran | Non | Oui |

## Utilisateurs et parcours

Le public est un petit groupe d'amis. Tous les membres ont les mêmes droits, sauf le créateur d'un serveur, qui le gère. L'accès à Estompe est accordé dans permission.plbls.fr (voir Fonctionnel v0).

1. Se connecter via security.plbls.fr avec son compte Keycloak (existant ou créé à ce moment, e-mail vérifié), puis, si la permission `estompe:access` lui a été accordée dans permission.plbls.fr, activer son compte Estompe en choisissant un pseudo
2. Créer un serveur, ou rejoindre un serveur existant
3. Écrire dans un salon texte, ou rejoindre un salon vocal
4. Envoyer un message privé à un ami

## Fonctionnel v0

La v0 couvre les comptes, la messagerie, les serveurs et les deux types de salons.

- Comptes : l'identité vient de Keycloak (connexion via security.plbls.fr), mais le compte Estompe est distinct : il se crée à la première connexion en choisissant un pseudo, à condition d'avoir la permission `estompe:access`. Un utilisateur déjà présent dans Keycloak n'a donc rien de plus à faire que se connecter.
- Permission : le service interroge permission.plbls.fr (`POST /v1/check`, Bearer de service, cache 30 s) ; sans la permission, ou tant que le pseudo n'est pas choisi, toute route du chat répond 403. Les accès (attribution de rôles) se gèrent dans l'interface de permission.
- Messages privés entre deux utilisateurs
- Serveurs : création, invitation par lien, liste des salons
- Salons texte : messages avec historique illimité
- Salons vocaux : rejoindre et quitter, audio en temps réel, 10 participants maximum par salon
- Chat écrit :
  - Gras, italique, barré, code en ligne et en bloc, citations, listes
  - Spoilers et aperçu des liens
  - Images et fichiers de 10 Mo maximum, compressés côté client
- Réactions emoji sous un message
- Mentions @pseudo avec notification
- Modifier et supprimer ses propres messages

## Fonctionnel v1

La v1 reprend la v0 et ajoute la vidéo.

- Caméra dans un salon vocal, activable et désactivable à tout moment
- Partage d'écran dans un salon vocal, avec choix de la fenêtre ou de l'écran
- Le moteur vocal de la v0 est réutilisé, sans refonte

## Plateformes

Un seul code Svelte 5 (SPA, build Vite) alimente les trois plateformes, avec la même priorité pour Windows et mobile.

| Plateforme | Technologie | Points d'attention |
| --- | --- | --- |
| Web | Svelte 5 + Vite | Même build que la PWA |
| Windows | Tauri 2, sur WebView2 | Binaire léger ; signature de code à prévoir pour éviter l'avertissement SmartScreen |
| Mobile | PWA installable | Notifications à valider sur iOS, qui exige une PWA installée |

## Design

Le design reprend la direction artistique de plbls.fr, gérée à un seul endroit et partagée par le site et l'app.

- Librairie commune `@plbls/ui` (dépôt `plbls-app/plbls-ui`, v0.3.0 au moment de l'écriture) : https://git.plbls.fr/plbls-app/plbls-ui/releases
- Tokens (couleurs, typographie, rayon, durées, marges, espacements) en variables CSS (`tokens.css`)
- Composants en feuille CSS à classes `plbls-*` (BEM) et web components en JS vanilla, indépendants de tout framework : utilisables tels quels dans l'app Svelte
- Version épinglée dans l'app, mise à jour volontaire
- Polices Bricolage Grotesque et Instrument Sans auto-hébergées, sans requête vers Google
- Rose surligneur pour l'onglet de salon actif, les mentions et les liens au survol
- Pas de mode sombre, DA unique
- Déjà dans le kit : textarea, select, checkbox, toggle, modal, toast, tabs, sidebar, avatar, badge, dropzone, tooltip
- À vérifier ou ajouter au kit : zone de saisie de chat (multi-ligne à hauteur automatique), liste de messages, indicateur vocal

## Performance

Ces cibles sont des points de départ, à valider par mesure sur une machine Windows réelle.

| Indicateur | Cible | Mesure |
| --- | --- | --- |
| RAM au repos, app Windows | Moins de 150 Mo | Gestionnaire des tâches, tous processus confondus |
| RAM en salon vocal | Moins de 250 Mo | Gestionnaire des tâches |
| CPU au repos | Proche de 0 % | Gestionnaire des tâches |
| Démarrage à froid | Moins de 2 s | Chronomètre, jusqu'à la liste des salons |
| Taille du binaire Windows | Moins de 20 Mo | Fichier installé |
| Envoi d'un message | Moins de 200 ms | Réseau domestique |

## Architecture technique

Le serveur API en Go et le vocal via SFU sont retenus ; la base de données reste une proposition.

| Composant | Rôle | Technologie |
| --- | --- | --- |
| Front web, PWA et Windows | Interface, chat, vocal côté client | Svelte 5 + Vite, PWA via vite-plugin-pwa ; Tauri 2 pour Windows |
| Authentification | Connexion et inscription via security.plbls.fr, puis Keycloak  | Keycloak (existant, non modifié) |
| Serveur API et temps réel | Serveurs, messages, présence ; identité obtenue en transmettant le cookie de session ou le Bearer à `GET /auth/me` du gateway (résultat gardé 30 s en cache) ; aucune route n'est ajoutée au gateway | Go, WebSocket |
| Base de données | Serveurs, messages, historique | PostgreSQL (proposition) |
| Serveur vocal (SFU) | Relais audio, puis vidéo en v1 | LiveKit ou mediasoup, auto-hébergé |
| Stockage de fichiers | Images et fichiers du chat | Disque du VPS (volume Docker), 20 Go maximum : au-delà, les plus anciens fichiers sont supprimés et leur message affiche « fichier expiré » |
| Hébergement | Tous les services | VPS, stacks déployées avec Dokploy à partir d'images Docker construites par la CI Gitea (jamais depuis le dépôt) |

## Sécurité et confidentialité

Les exigences minimales sont fixées ici ; aucun audit externe n'est prévu en v0.

- Authentification déléguée à Keycloak : aucun mot de passe stocké par l'appli ; point d'entrée unique security.plbls.fr (auth-gateway) : web et PWA en BFF avec cookie de session HttpOnly, app Windows en PKCE natif (client plbls-desktop) ; le service n'a aucune route dans le gateway et ne voit jamais de token Keycloak : il transmet le cookie ou le Bearer reçu à `GET /auth/me` (cookie de session sur `.plbls.fr`, donc le service doit être sous un sous-domaine de plbls.fr)
- HTTPS et WSS partout ; le vocal WebRTC est chiffré par défaut
- Markdown rendu avec sanitisation (DOMPurify) ; aucun HTML brut accepté dans les messages
- Fichiers : type vérifié côté serveur, taille maximale de 10 Mo, stockage plafonné à 20 Go avec suppression des plus anciens
- Un message n'est accessible qu'à ses destinataires, contrôle fait côté serveur
- Chiffrement de bout en bout : non prévu en v0 ni en v1, reporté à la v2

## Hors périmètre

Ces éléments ne sont pas livrés en v0 ni en v1.

- Application mobile native dans un store : la PWA suffit
- Bots et intégrations tierces
- Modération avancée et rôles fins par salon
- Chiffrement de bout en bout (reporté à la v2)
- Mode sombre
- Recherche dans l'historique des messages (hypothèse à valider)

## Risques

Le risque principal est la consommation de RAM de WebView2 ; il se mesure dès le premier prototype.

| Risque | Impact | Parade |
| --- | --- | --- |
| RAM de WebView2 supérieure à la cible | Élevé | Mesure dès le prototype, puis avant chaque jalon |
| Vocal instable au-delà de quelques personnes | Moyen | SFU dès la v0 |
| Avertissement SmartScreen sur l'exécutable Windows | Moyen | Avertissement accepté, pas de certificat |
| Notifications iOS limitées pour la PWA | Moyen | Documenter la limite, prévoir une alternative |
| Développement seul sur front, back, vocal et design | Élevé | Périmètre v0 strict, jalons courts |
| Faille XSS via le markdown | Élevé | Sanitisation et tests dédiés |

## Questions ouvertes

- [x] Nombre maximal de personnes par salon vocal en v0 : 10
- [x] Taille maximale des fichiers envoyés dans le chat : 10 Mo
- [x] Accès : compte Keycloak + permission `estompe:access` (permission.plbls.fr) ; compte Estompe distinct du compte Keycloak, sans modification de Keycloak
- [x] Hébergement : VPS avec Dokploy ; stockage des fichiers sur le disque du VPS (20 Go max, suppression des plus anciens), pas de S3 pour le moment
- [x] Signature de code Windows : pas de certificat, l'avertissement SmartScreen est accepté
- [x] Chiffrement de bout en bout : reporté à la v2

## Jalons

Cinq jalons, sans dates fixées. Chaque jalon se valide par son critère de sortie.

| Jalon | Contenu | Critère de sortie |
| --- | --- | --- |
| J1 Socle | Comptes, connexion, design system partagé, app Tauri vide | Un compte se crée, l'app démarre, la RAM est mesurée |
| J2 Messagerie | Messages privés, serveurs, salons texte, chat formaté | Envoi sous 200 ms, markdown sanitisé |
| J3 Vocal | Salons vocaux via SFU | Vocal stable avec un groupe complet |
| J4 Finitions v0 | Réactions, mentions, modifier et supprimer, PWA mobile | Cibles de performance atteintes |
| J5 v1 | Caméra et partage d'écran | Partage fluide, RAM toujours sous la cible |
