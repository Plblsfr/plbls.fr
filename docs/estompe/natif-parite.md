# Parité de l'app native

L'app de bureau native (`apps/estompe/natif`, Rust et Slint, sans webview) est écrite par lots à côté de l'app Windows en Tauri. Elle reste
en bêta tant qu'elle ne couvre pas toutes les fonctions de l'app actuelle. Cette matrice dit ce qui manque : une ligne par composant Svelte de
`web/estompe/src/`, par bloc, par section et par fonction propre à l'app Windows. Épopée : [#271](https://github.com/Plblsfr/plbls.fr/issues/271).

## États

- `absent` : rien dans l'app native.
- `en cours` : une PR fusionnée couvre une partie de l'élément.
- `fait` : l'app native couvre l'élément.
- `écart assumé` : l'app native ne reprend pas l'élément tel quel ; la note dit pourquoi et ce qui le remplace.

## Mise à jour

- Une PR qui ajoute une fonction visible au front web (composant, bloc, section, fonction de l'app Windows) ajoute sa ligne ici, à l'état `absent`,
  et ouvre l'issue native correspondante (label `natif`), citée dans la colonne « Issue ».
- Une PR de l'app native met à jour l'état des lignes qu'elle couvre.
- Un composant supprimé ou renommé : sa ligne suit dans la même PR.
- Un écart assumé se décide dans une issue ; la note en donne la raison.

## Format

- Cinq colonnes : élément, lot, issue, état, note.
- Un chemin entre accents graves en première colonne est vérifié : relatif à `web/estompe/src/`, ou à la racine du dépôt s'il commence par
  `apps/` ou `web/` ; un dossier se termine par `/`. Dans une note, un chemin qui commence par `apps/` ou `web/` est vérifié aussi.
- La ligne d'un bloc ou d'une section porte le lot où l'ensemble est complet ; ses composants peuvent arriver plus tôt.
- Issue : celle qui livre l'élément quand elle existe, sinon celle du lot.

## Vérification

```sh
node scripts/estompe/natif-parite.mjs
```

ou `just estompe-natif-parite` ; `scripts/estompe/check.sh` le lance aussi. Le script échoue si un composant `.svelte` de `web/estompe/src/`,
un bloc ou une section n'a pas de ligne, si une ligne désigne un fichier disparu, si une ligne est en double ou si un état est hors de la liste.
Sinon il affiche le nombre de composants et leur répartition par état.

## Lots

- Lot 0, [#272](https://github.com/Plblsfr/plbls.fr/issues/272) : socle (squelette, crates client et plateforme, composants de base, configuration).
- Lot 1, [#273](https://github.com/Plblsfr/plbls.fr/issues/273) : connexion et coquille.
- Lot 2, [#274](https://github.com/Plblsfr/plbls.fr/issues/274) : messagerie, lecture.
- Lot 3, [#275](https://github.com/Plblsfr/plbls.fr/issues/275) : messagerie, écriture.
- Lot 4, [#276](https://github.com/Plblsfr/plbls.fr/issues/276) : vocal, audio.
- Lot 5, [#277](https://github.com/Plblsfr/plbls.fr/issues/277) : caméra et partage d'écran.
- Lot 6, [#278](https://github.com/Plblsfr/plbls.fr/issues/278) : intégration au bureau.
- Lot 7, [#279](https://github.com/Plblsfr/plbls.fr/issues/279) : jeu (overlays, replay, mesures).
- Lot 8, [#280](https://github.com/Plblsfr/plbls.fr/issues/280) : réglages, profil, administration, modération.
- Lot 9, [#281](https://github.com/Plblsfr/plbls.fr/issues/281) : médias (vidéos, regarder ensemble, musique).
- Lot 10, [#282](https://github.com/Plblsfr/plbls.fr/issues/282) : sections (Entracte, Régie, Sablier, Sillon).
- Lot 11, [#283](https://github.com/Plblsfr/plbls.fr/issues/283) : Draw2gether, effets caméra, finitions, paquet Linux.

## Écarts assumés

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `blocks/regarder-ensemble/youtube.ts` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | écart assumé | seule exception à « sans webview » : une vue `wry` créée à la demande, derrière une feature cargo ; sinon le lien s'ouvre dans le navigateur |
| `blocks/notifications/push.ts` | 6 | [#278](https://github.com/Plblsfr/plbls.fr/issues/278) | écart assumé | push web (abonnement VAPID) sans objet : l'app native affiche les notifications de l'OS |
| `web/estompe/public/push-sw.js` | 6 | [#278](https://github.com/Plblsfr/plbls.fr/issues/278) | écart assumé | service worker du push web, sans objet |

## Cœur

### Application et vues

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `App.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | absent | aiguillage entre connexion, refus, activation, sections et coquille ; thème, densité, pastille de mentions |
| `views/LoginView.svelte` | 1 | [#293](https://github.com/Plblsfr/plbls.fr/issues/293) | fait | écran de connexion, attente du navigateur annulable, erreurs (`apps/estompe/natif/ui/entree/connexion.slint`) ; sans liens de téléchargement des apps |
| `views/DeniedView.svelte` | 1 | [#293](https://github.com/Plblsfr/plbls.fr/issues/293) | fait | compte sans la permission d'utiliser Estompe (`apps/estompe/natif/ui/entree/acces.slint`) |
| `views/ActivateView.svelte` | 1 | [#293](https://github.com/Plblsfr/plbls.fr/issues/293) | fait | choix du pseudo à l'activation, vérifié avant l'envoi (`apps/estompe/natif/ui/entree/acces.slint`) |
| `views/JoinView.svelte` | 1 | [#293](https://github.com/Plblsfr/plbls.fr/issues/293) | fait | invitation saisie ou passée au lancement (`--invitation`) ; les liens `estompe://` ouverts par le système restent à faire |
| `views/Shell.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | absent | coquille : rail, panneau latéral, zone principale, modals |
| `views/HomeView.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | absent | accueil : à rattraper, vocaux en cours, contacts en ligne |
| `views/SectionsShell.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent | coquille d'un compte limité aux sections |
| `views/AdminView.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | coquille d'un compte limité à l'administration |

### Composants de base

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `components/Icon.svelte` | 0 | [#288](https://github.com/Plblsfr/plbls.fr/issues/288) | fait | jeu d'icônes généré depuis `Icon.svelte` (`scripts/estompe/natif-da.mjs`) |
| `components/Avatar.svelte` | 0 | [#288](https://github.com/Plblsfr/plbls.fr/issues/288) | fait | initiales, image, halo du locuteur ; la pastille de présence arrive avec #297 |
| `components/Loader.svelte` | 0 | [#288](https://github.com/Plblsfr/plbls.fr/issues/288) | fait | indicateur de chargement |
| `components/Modal.svelte` | 0 | [#288](https://github.com/Plblsfr/plbls.fr/issues/288) | fait | fermeture par Échap, la croix ou le fond ; le focus reste dans le modal |
| `components/ConfirmModal.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | absent | confirmation, avec saisie ou case à cocher ; hors de la liste de #288, se construit sur le modal natif |
| `components/ContextMenu.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | absent | menu contextuel commun ; hors de la liste de #288 |

### Coquille, membres, mise à jour forcée

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `components/ServerRail.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | absent | rail des serveurs, ordre, non-lus |
| `components/ServerIcon.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | absent |  |
| `components/SidePanel.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | absent | salons, messages privés, barre du vocal et de l'utilisateur ; commandes du vocal au lot 4 |
| `components/Marge.svelte` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | absent | panneau de droite redimensionnable : membres, épinglés |
| `components/MembersPanel.svelte` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | absent | membres, présence, activité |
| `components/RoleBadge.svelte` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | absent | étiquette « Bot » ou « Créateur » |
| `components/ForcedUpdate.svelte` | 1 | [#298](https://github.com/Plblsfr/plbls.fr/issues/298) | en cours | écran bloquant sur refus 426 (`apps/estompe/natif/ui/entree/mise-a-jour.slint`), lien vers la page du serveur ; ni téléchargement ni installation dans l'app tant que l'installeur natif n'existe pas |

### Messagerie, lecture

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `components/ChatView.svelte` | 2 | [#274](https://github.com/Plblsfr/plbls.fr/issues/274) | absent | en-tête, liste et zone de saisie d'une conversation ; saisie au lot 3 |
| `components/MessageList.svelte` | 2 | [#299](https://github.com/Plblsfr/plbls.fr/issues/299) | absent | liste virtualisée, accusés de lecture |
| `components/MessageItem.svelte` | 2 | [#301](https://github.com/Plblsfr/plbls.fr/issues/301) | absent | lecture ; édition, réponse et actions au lot 3 |
| `components/AttachmentView.svelte` | 2 | [#303](https://github.com/Plblsfr/plbls.fr/issues/303) | absent | images, GIF, fichiers ; vidéos au lot 9 |
| `components/Lightbox.svelte` | 2 | [#303](https://github.com/Plblsfr/plbls.fr/issues/303) | absent | visionneuse plein écran |
| `components/VoiceMessage.svelte` | 2 | [#303](https://github.com/Plblsfr/plbls.fr/issues/303) | absent | lecture d'un message vocal, vitesse, transcription |
| `components/PollCard.svelte` | 2 | [#304](https://github.com/Plblsfr/plbls.fr/issues/304) | absent | sondage affiché ; le vote est une écriture (lot 3) |
| `components/ConvPanel.svelte` | 2 | [#274](https://github.com/Plblsfr/plbls.fr/issues/274) | absent | messages épinglés d'une conversation |
| `components/SearchModal.svelte` | 2 | [#274](https://github.com/Plblsfr/plbls.fr/issues/274) | absent | recherche de messages et palette « aller à » (Ctrl+K) |

### Messagerie, écriture

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `components/Composer.svelte` | 3 | [#275](https://github.com/Plblsfr/plbls.fr/issues/275) | absent | saisie, brouillons, mentions, pièces jointes, réponse |
| `components/EditAttachments.svelte` | 3 | [#275](https://github.com/Plblsfr/plbls.fr/issues/275) | absent | pièces jointes d'un message en cours d'édition |
| `components/PollModal.svelte` | 3 | [#275](https://github.com/Plblsfr/plbls.fr/issues/275) | absent | création d'un sondage |
| `components/VoiceNote.svelte` | 3 | [#275](https://github.com/Plblsfr/plbls.fr/issues/275) | absent | enregistrement d'un message vocal |
| `components/NewDMModal.svelte` | 3 | [#275](https://github.com/Plblsfr/plbls.fr/issues/275) | absent | nouvelle conversation privée ou de groupe |

### Vocal, caméra, partage d'écran

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `components/Quality.svelte` | 4 | [#276](https://github.com/Plblsfr/plbls.fr/issues/276) | absent | qualité de la connexion vocale |
| `components/VoiceTimer.svelte` | 4 | [#276](https://github.com/Plblsfr/plbls.fr/issues/276) | absent | durée passée en vocal |
| `components/MediaPicker.svelte` | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | absent | choix de la source, de la qualité et de la caméra avant un partage |
| `components/CameraPreview.svelte` | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | absent | aperçu de la caméra |
| `components/CameraOverlayControls.svelte` | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | absent | incrustation de la caméra dans le partage d'écran |
| `components/CameraOverlayPreview.svelte` | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | absent | aperçu de l'incrustation |
| `components/CameraFxControls.svelte` | 11 | [#283](https://github.com/Plblsfr/plbls.fr/issues/283) | absent | arrière-plan et effets de la caméra |

### Médias

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `components/NowPlaying.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent | musique partagée d'un salon vocal |
| `components/MiniProgress.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent | barre de progression des mini-lecteurs ; aussi utilisée par Sillon (lot 10) |

### Réglages, profil, administration, modération, mises à jour

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `components/SettingsModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | réglages : compte, apparence, vocal, caméra, raccourcis, pages de la plateforme |
| `components/SettingsDebug.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | journal de débogage ; le journal lui-même vient de #291 |
| `components/ProfileModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | carte de profil |
| `components/ServerSettingsModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | réglages d'un serveur |
| `components/RolesSettings.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/ChannelPerms.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | permissions d'un salon |
| `components/ChannelModModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | réglages et modération d'un salon |
| `components/IconCrop.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | recadrage de l'icône d'un serveur |
| `components/CreateChannelModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/CreateJoinModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | créer ou rejoindre un serveur |
| `components/ModerationModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | expulser, bannir, exclure, avertir, renommer |
| `components/AdminSpace.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | administration d'Estompe |
| `components/UpdateNotice.svelte` | 6 | [#278](https://github.com/Plblsfr/plbls.fr/issues/278) | absent | mise à jour disponible, non forcée |
| `components/UpdaterPanel.svelte` | 6 | [#278](https://github.com/Plblsfr/plbls.fr/issues/278) | absent | version installée et installation d'une mise à jour |

### Administration des accès

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `components/acces/AccesAdmin.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | onglets comptes, rôles, groupes, catalogue, journal |
| `components/acces/Attente.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | chargement ou erreur d'une liste |
| `components/acces/CatalogueVue.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/ChoixCompte.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/ChoixRessource.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/Comptes.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/FicheCompte.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/FormLiaison.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/FormRegle.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/Groupes.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/JournalAcces.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/LigneJournal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/ListeLiaisons.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/ListeRegles.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/NomRessource.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/RoleEditeur.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/acces/Roles.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |

## Blocs

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `blocks/activite-statut/` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | absent | statut et activité affichés ; détection du jeu en cours au lot 7 |
| `blocks/apercu-liens/` | 2 | [#304](https://github.com/Plblsfr/plbls.fr/issues/304) | absent |  |
| `blocks/avatars/` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | absent | images de profil ; les initiales viennent de #288 |
| `blocks/badges/` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | affichage aux lots 1 et 2, administration au lot 8 |
| `blocks/draw2gether/` | 11 | [#283](https://github.com/Plblsfr/plbls.fr/issues/283) | absent |  |
| `blocks/emojis/` | 3 | [#275](https://github.com/Plblsfr/plbls.fr/issues/275) | absent | lecture au lot 2 (#302), sélecteur au lot 3 |
| `blocks/gifs/` | 3 | [#275](https://github.com/Plblsfr/plbls.fr/issues/275) | absent | lecture au lot 2 (#303), sélecteur au lot 3 |
| `blocks/musique/` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent |  |
| `blocks/notifications/` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | notifications de l'OS au lot 6, réglages au lot 8 ; push web : écart assumé |
| `blocks/regarder-ensemble/` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent | lecteur YouTube : écart assumé |
| `blocks/signalements/` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `blocks/transcription/` | 2 | [#303](https://github.com/Plblsfr/plbls.fr/issues/303) | absent | transcription des messages vocaux, sans composant propre |
| `blocks/videos/` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent |  |
| `blocks/vocal/` | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | absent | audio au lot 4, caméra et partage au lot 5 |

### Composants des blocs

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `blocks/apercu-liens/LinkCard.svelte` | 2 | [#304](https://github.com/Plblsfr/plbls.fr/issues/304) | absent | aperçu d'un lien, carte Sillon |
| `blocks/badges/BadgeIcon.svelte` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | absent |  |
| `blocks/badges/BadgeInline.svelte` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | absent | badges à côté d'un pseudo (membres, messages) |
| `blocks/badges/BadgeRow.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | badges de la carte de profil |
| `blocks/badges/BadgeAdmin.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `blocks/draw2gether/Draw2Gether.svelte` | 11 | [#283](https://github.com/Plblsfr/plbls.fr/issues/283) | absent |  |
| `blocks/draw2gether/DrawCanvas.svelte` | 11 | [#283](https://github.com/Plblsfr/plbls.fr/issues/283) | absent |  |
| `blocks/draw2gether/DrawEnd.svelte` | 11 | [#283](https://github.com/Plblsfr/plbls.fr/issues/283) | absent |  |
| `blocks/draw2gether/DrawLobby.svelte` | 11 | [#283](https://github.com/Plblsfr/plbls.fr/issues/283) | absent |  |
| `blocks/draw2gether/DrawPlay.svelte` | 11 | [#283](https://github.com/Plblsfr/plbls.fr/issues/283) | absent |  |
| `blocks/emojis/EmojiPicker.svelte` | 3 | [#275](https://github.com/Plblsfr/plbls.fr/issues/275) | absent |  |
| `blocks/gifs/GifPicker.svelte` | 3 | [#275](https://github.com/Plblsfr/plbls.fr/issues/275) | absent |  |
| `blocks/musique/MusicDock.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent | lecteur de la musique du salon |
| `blocks/musique/MusicMini.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent |  |
| `blocks/notifications/SettingsNotifications.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | réglages des notifications |
| `blocks/regarder-ensemble/WatchPicker.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent | choix du salon où lancer une séance |
| `blocks/regarder-ensemble/WatchTogether.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent | séance synchronisée, salon cinéma |
| `blocks/regarder-ensemble/Watchers.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent | spectateurs d'un partage |
| `blocks/signalements/ReportModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | signalement avec captures |
| `blocks/signalements/SettingsReports.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | mes signalements |
| `blocks/signalements/AdminReports.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `blocks/videos/VideoPlayer.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent | lecteur des vidéos jointes |
| `blocks/videos/VideosView.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent | salon vidéos ou musique |
| `blocks/videos/VideoCard.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent |  |
| `blocks/videos/VideoLists.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent |  |
| `blocks/videos/VideoSessions.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent |  |
| `blocks/videos/SoloDock.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent |  |
| `blocks/videos/SoloMini.svelte` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent |  |
| `blocks/vocal/VoiceChannelView.svelte` | 4 | [#276](https://github.com/Plblsfr/plbls.fr/issues/276) | absent | salon vocal ; scène vidéo au lot 5 |
| `blocks/vocal/VoiceControls.svelte` | 4 | [#276](https://github.com/Plblsfr/plbls.fr/issues/276) | absent | micro, casque, raccrocher ; boutons caméra et partage au lot 5 |
| `blocks/vocal/StageTile.svelte` | 4 | [#276](https://github.com/Plblsfr/plbls.fr/issues/276) | absent | tuile d'un participant ; flux caméra et écran au lot 5 |

## Sections

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `sections/entracte/` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent | films et séries |
| `sections/regie/` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent | téléchargements et dépôts |
| `sections/sablier/` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent | statistiques, récaps, trophées |
| `sections/sillon/` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent | musique |

### Entracte

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `sections/entracte/FilmsSpace.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/FilmsNav.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/FilmsLibrary.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/FilmDetail.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/FilmPlayer.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent | lecteur HLS, sous-titres, passer l'intro, diffusion sur une télé |
| `sections/entracte/FilmPoster.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/FilmNoAccess.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent | salon cinéma sans accès au titre |
| `sections/entracte/EntractePoster.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/EntracteSearch.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/EntracteRequests.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/RequestForm.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/EntracteAdmin.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/AcceptModal.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/ArrAdd.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/ArrCatalog.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/ArrDownloads.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/ArrImport.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/ArrMissing.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/ArrReleases.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/entracte/ArrSheet.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |

### Régie

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `sections/regie/RegieSpace.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/regie/RegieNav.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/regie/RegieHealth.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/regie/RegieDownloads.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/regie/RegieTorrent.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/regie/RegieLimits.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/regie/RegieDeposits.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/regie/RegieDeposit.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/regie/RegieImport.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/regie/RegieSheet.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |

### Sablier

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `sections/sablier/SablierSpace.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierNav.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierJeu.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierCarte.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierPeriode.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierSeries.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierTrophees.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierMedaille.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierRecaps.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierRecap.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierProfil.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierPersonne.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierPartage.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierDonnees.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierAnnonce.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierErreur.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierAdmin.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierAdminJournal.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierAdminPersonnes.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierAdminTotaux.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierAdminTrophees.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sablier/SablierTropheeEdition.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |

### Sillon

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `sections/sillon/SillonSpace.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonNav.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonHome.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonSearch.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonAlbums.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonAlbum.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonArtists.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonArtist.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonTrack.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonTracks.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonPlaylist.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonChoixPlaylist.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonQueue.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent | file de lecture |
| `sections/sillon/SillonMini.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent | mini-lecteur |
| `sections/sillon/SillonHistory.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonLike.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonCover.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonMosaique.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonAdd.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonAdditions.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonAdmin.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |
| `sections/sillon/SillonNoAccess.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent |  |

## App Windows

Fonctions de `apps/estompe/windows/` et de `apps/estompe/crates/core/` qui n'existent pas dans le front web.

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| Requêtes HTTP faites par Rust (`native_fetch`) | 0 | [#286](https://github.com/Plblsfr/plbls.fr/issues/286) | fait | crate `estompe-client` (reqwest) ; référence : `apps/estompe/crates/core/src/commands.rs` |
| Journal de débogage écrit sur disque | 0 | [#291](https://github.com/Plblsfr/plbls.fr/issues/291) | fait | journal tournant et expurgé de la crate `estompe-client` ; référence : `apps/estompe/crates/core/src/debuglog.rs` |
| Connexion par le navigateur du système (PKCE, retour sur un serveur local), jeton de rafraîchissement dans le coffre de l'OS | 1 | [#292](https://github.com/Plblsfr/plbls.fr/issues/292) | absent | `apps/estompe/windows/src/auth.ts`, `apps/estompe/crates/core/src/callback.rs` |
| Conversion d'une vidéo en H.264 avant l'envoi (ffmpeg embarqué) | 3 | [#275](https://github.com/Plblsfr/plbls.fr/issues/275) | absent | `apps/estompe/windows/src-tauri/src/video.rs` |
| Partage d'écran natif : liste des écrans et fenêtres avec miniatures | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | absent | `apps/estompe/windows/src/capture.ts`, `apps/estompe/windows/src-tauri/src/capture.rs` |
| Partage d'écran natif : capture d'une fenêtre | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | absent | `apps/estompe/windows/src-tauri/src/capture_fenetre.rs` |
| Partage d'écran natif : publication dans le salon par Rust (LiveKit) | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | absent | `apps/estompe/windows/src-tauri/src/screen_native.rs` |
| Son du partage (fenêtre, ou système sans Estompe) par WASAPI | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | absent | `apps/estompe/windows/src-tauri/src/son_wasapi.rs` |
| Installeur NSIS | 6 | [#278](https://github.com/Plblsfr/plbls.fr/issues/278) | absent | `apps/estompe/windows/src-tauri/tauri.conf.json`, `apps/estompe/windows/scripts/release-windows.sh`, `apps/estompe/windows/scripts/check-installer.sh` |
| Mises à jour signées, installation et redémarrage | 6 | [#278](https://github.com/Plblsfr/plbls.fr/issues/278) | absent | `apps/estompe/windows/src/updates.ts`, `apps/estompe/windows/src/UpdateSettings.svelte` |
| Instance unique : un second lancement ramène la fenêtre | 6 | [#278](https://github.com/Plblsfr/plbls.fr/issues/278) | absent | `apps/estompe/windows/src-tauri/src/lib.rs` |
| Icône de la zone de notification ; la fenêtre se masque à la fermeture | 6 | [#278](https://github.com/Plblsfr/plbls.fr/issues/278) | absent | `apps/estompe/windows/src-tauri/src/lib.rs` |
| Toasts natifs avec « Répondre » et « Marquer comme lu » | 6 | [#278](https://github.com/Plblsfr/plbls.fr/issues/278) | absent | `apps/estompe/windows/src/notifications.ts`, `apps/estompe/windows/src-tauri/src/toast.rs` |
| Pastille de mentions sur l'icône de la barre des tâches | 6 | [#278](https://github.com/Plblsfr/plbls.fr/issues/278) | absent | `apps/estompe/windows/src/notifications.ts` |
| Appuyer pour parler global, hors focus | 6 | [#278](https://github.com/Plblsfr/plbls.fr/issues/278) | absent | `apps/estompe/windows/src/ptt-global.ts` |
| Jeu au premier plan : détection, jeux connus, ajoutés, exclus | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src-tauri/src/jeux/`, `apps/estompe/windows/src-tauri/src/foreground.rs` |
| Jeu en cours montré aux autres | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src/SettingsShare.svelte` |
| Overlay des participants du vocal sur le jeu | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src/Overlay.svelte`, `apps/estompe/windows/src/overlay.ts`, `apps/estompe/windows/src/overlay.svelte.ts`, `apps/estompe/windows/src/SettingsOverlay.svelte` |
| Overlay natif des participants (Slint, expérimental) | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src-tauri/src/overlay_natif/` |
| Overlay des messages privés et mentions, avec réponse rapide | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src/MsgOverlay.svelte`, `apps/estompe/windows/src/msg-overlay.svelte.ts` |
| Instant replay : tampon du jeu, encodage matériel, micro, sauvegarde par raccourci | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src-tauri/src/replay/`, `apps/estompe/windows/src/replay.ts`, `apps/estompe/windows/src/replay.svelte.ts` |
| Instant replay : réglages | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src/SettingsReplay.svelte`, `apps/estompe/windows/src-tauri/src/replay/reglages.rs` |
| Instant replay : indicateur sur le jeu | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src/ReplayOverlay.svelte`, `apps/estompe/windows/src/replay-overlay.svelte.ts` |
| Galerie des clips (espace « Clips »), stockage et quota | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src/ReplayGalerie.svelte`, `apps/estompe/windows/src/ReplayNav.svelte`, `apps/estompe/windows/src-tauri/src/replay/galerie.rs` |
| Envoi d'un clip dans une conversation | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src/ReplayEnvoi.svelte` |
| Éditeur de clips : montage et export | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src/ReplayEditeur.svelte`, `apps/estompe/windows/src/montage.ts`, `apps/estompe/windows/src/montage.svelte.ts`, `apps/estompe/windows/src-tauri/src/replay/montage.rs` |
| Mesures : relevés de l'app et du système | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src/mesures.ts`, `apps/estompe/windows/src/mesures.svelte.ts`, `apps/estompe/windows/src/SettingsMesures.svelte`, `apps/estompe/windows/src-tauri/src/mesures.rs`, `apps/estompe/windows/scripts/synthese-mesures.mjs` |
| Options de WebView2 (économie de mémoire, purge des données à chaque version) | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src-tauri/src/webview2_options.rs`, `apps/estompe/windows/src-tauri/src/webview_data.rs` ; propre à la webview, à trancher au lot 7 |
| Capture de la fenêtre d'Estompe pour un signalement | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | `apps/estompe/windows/src-tauri/src/capture.rs` |
| Déblocage des fonctions alpha (7 clics sur la version) | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | `apps/estompe/windows/src/UpdateSettings.svelte` |
