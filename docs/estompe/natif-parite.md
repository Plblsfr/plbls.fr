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
| `App.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | en cours | aiguillage entre connexion, refus, activation et coquille, retour à la connexion à la session perdue (`apps/estompe/natif/src/coquille/cycle.rs`) ; restent la coquille limitée aux sections et la densité |
| `views/LoginView.svelte` | 1 | [#293](https://github.com/Plblsfr/plbls.fr/issues/293) | fait | écran de connexion, attente du navigateur annulable, erreurs (`apps/estompe/natif/ui/entree/connexion.slint`) ; sans liens de téléchargement des apps |
| `views/DeniedView.svelte` | 1 | [#293](https://github.com/Plblsfr/plbls.fr/issues/293) | fait | compte sans la permission d'utiliser Estompe (`apps/estompe/natif/ui/entree/acces.slint`) |
| `views/ActivateView.svelte` | 1 | [#293](https://github.com/Plblsfr/plbls.fr/issues/293) | fait | choix du pseudo à l'activation, vérifié avant l'envoi (`apps/estompe/natif/ui/entree/acces.slint`) |
| `views/JoinView.svelte` | 1 | [#293](https://github.com/Plblsfr/plbls.fr/issues/293) | fait | invitation saisie ou passée au lancement (`--invitation`) ; les liens `estompe://` ouverts par le système restent à faire |
| `views/Shell.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | en cours | rail, panneau repliable, zone principale avec bandeau « Reconnexion en cours… », dernière vue rouverte, emplacement de la marge (`apps/estompe/natif/ui/coquille/coquille.slint`) ; recherche et palette par Ctrl+K (#321, `apps/estompe/natif/ui/coquille/recherche.slint`) ; restent les modals (création, réglages, profil), le Dock et la barre d'onglets sous 900 px |
| `views/HomeView.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | en cours | salut, date, résumé, à rattraper avec « Tout marquer lu » (#322), contacts en ligne avec leur occupation (#322), vocaux en cours avec « Rejoindre » ou « Ouvrir » (#329), pour commencer (`apps/estompe/natif/ui/coquille/accueil.slint`), palette par le bouton de recherche de l'en-tête ou Ctrl+K (#321) ; restent les avatars des participants des vocaux en cours |
| `views/SectionsShell.svelte` | 10 | [#282](https://github.com/Plblsfr/plbls.fr/issues/282) | absent | coquille d'un compte limité aux sections |
| `views/AdminView.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | coquille d'un compte limité à l'administration |

### Composants de base

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `components/Icon.svelte` | 0 | [#288](https://github.com/Plblsfr/plbls.fr/issues/288) | fait | jeu d'icônes généré depuis `Icon.svelte` (`scripts/estompe/natif-da.mjs`) |
| `components/Avatar.svelte` | 0 | [#288](https://github.com/Plblsfr/plbls.fr/issues/288) | fait | initiales, image (recadrée en carré, coins arrondis dans l'image), halo du locuteur, pastille de présence par formes (#297, `apps/estompe/natif/ui/composants/avatar.slint`) |
| `components/Loader.svelte` | 0 | [#288](https://github.com/Plblsfr/plbls.fr/issues/288) | fait | indicateur de chargement |
| `components/Modal.svelte` | 0 | [#288](https://github.com/Plblsfr/plbls.fr/issues/288) | fait | fermeture par Échap, la croix ou le fond ; le focus reste dans le modal |
| `components/ConfirmModal.svelte` | 1 | [#318](https://github.com/Plblsfr/plbls.fr/issues/318) | fait | confirmation commune sur le modal natif, action destructive, saisie à recopier exactement ou case à cocher, une demande remplace la précédente (annulée) (`apps/estompe/natif/ui/composants/confirmation.slint`, `apps/estompe/natif/src/confirmation.rs`) |
| `components/ContextMenu.svelte` | 1 | [#318](https://github.com/Plblsfr/plbls.fr/issues/318) | fait | menu contextuel commun : clic droit, touche Menu, Maj+F10 ou bouton (s'ouvre dessous, se retourne au-dessus en bas de fenêtre), gardé dans la fenêtre, flèches, Origine, Fin, Entrée, Échap (focus rendu à l'élément qui l'a ouvert), séparateurs, entrée dangereuse, coche, raccourci, entrée « bientôt » (`apps/estompe/natif/ui/composants/menu.slint`, `apps/estompe/natif/src/menu.rs`) ; écart : se ferme au clic, à la molette ou à la perte du focus, pas au redimensionnement de la fenêtre |

### Coquille, membres, mise à jour forcée

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `components/ServerRail.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | en cours | accueil, serveurs dans l'ordre retenu (glisser, Ctrl+Alt+↑/↓), trait de non-lus, pastille de mentions, entrées des sections, recherche globale (#321), indicateur « Connecté en vocal » (#329) (`apps/estompe/natif/ui/coquille/rail.slint`) ; « + » ne fait que rejoindre ; reste l'administration (lot 8) |
| `components/ServerIcon.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | fait | image du serveur téléchargée par le client (`apps/estompe/natif/src/coquille/pont.rs`), sinon initiales ; rail et en-tête du panneau |
| `components/SidePanel.svelte` | 1 | [#296](https://github.com/Plblsfr/plbls.fr/issues/296) | en cours | salons texte, esquisses et vocaux avec non-lus et mentions, messages privés triés par activité avec présence par formes, barre de l'utilisateur avec déconnexion (`apps/estompe/natif/ui/coquille/panneau.slint`) ; marque des conversations avec un brouillon (#314) ; menu contextuel d'un salon ou d'une conversation privée (« Marquer comme lu », « Copier le lien ») (#318) ; bouton « Nouveau message privé », menu de statut de la barre de l'utilisateur (en ligne, absent, ne pas déranger ; profil et signalement « bientôt ») et statut ou jeu sous le pseudo (#322) ; participants sous chaque salon vocal (parole, micro et casque coupés, coupures du serveur) et barre du vocal en cours avec ses commandes (#329, `apps/estompe/natif/ui/vocal/barre.slint`) ; un clic sur un salon vocal le rejoint ; photo de profil des participants, menu d'un participant avec « Déplacer vers » et « Déconnecter du vocal », durée du salon (#329) ; restent le glisser d'un participant, la création de salon et les séances à venir |
| `components/Marge.svelte` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | en cours | onglets « Membres » (sur un serveur) et « Épinglés » (dans une conversation), bouton de l'en-tête, Ctrl+. et bouton « Fermer la marge », ouverture retenue dans les préférences, onglet effectif comme `ongletEffectif` du web (`apps/estompe/natif/ui/coquille/membres.slint`) ; pas encore : redimensionnement, marge superposée sous 1200 px |
| `components/MembersPanel.svelte` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | en cours | liste virtualisée groupée par rôle affiché puis en ligne / hors ligne, compteur, filtre au-delà de 8 membres, couleur du rôle, exclusion, « Bot » et « Créateur », badges, jeu en cours, carte de profil au clic ou par Entrée, menu contextuel (« Profil », « Mentionner » dans un salon du serveur, « Copier le pseudo ») (#318) ; pas encore : message privé, rôles et modération du menu, temps passé en vocal, « Écoute » (musique), repli de la liste |
| `components/RoleBadge.svelte` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | fait | étiquette « Bot » ou « Créateur » (`apps/estompe/natif/ui/composants/badge-role.slint`) |
| `components/ForcedUpdate.svelte` | 6 | [#345](https://github.com/Plblsfr/plbls.fr/issues/345) | fait | écran bloquant sur refus 426 ou mise à jour imposée par le manifeste, après 120 s si l'utilisateur écrit (`apps/estompe/natif/ui/entree/mise-a-jour.slint`) : installe directement (progression, erreur, « Réessayer ») ; en cas d'échec, et sous Linux, lien vers la page du serveur |

### Messagerie, lecture

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `components/ChatView.svelte` | 2 | [#274](https://github.com/Plblsfr/plbls.fr/issues/274) | fait | en-tête avec bouton des épinglés et de recherche (#321), liste, « est en train d'écrire », lecture seule de la conversation Estompe ou sans droit d'écrire, zone de saisie (#314) (`apps/estompe/natif/ui/messagerie/vue.slint`) |
| `components/MessageList.svelte` | 2 | [#299](https://github.com/Plblsfr/plbls.fr/issues/299) | en cours | liste virtualisée à hauteurs mesurées, position gardée à l'ajout en tête, pages vers le haut et le bas, collage en bas, bouton « nouveaux messages », saut au premier non-lu, séparateurs de jour et de non-lus, saut vers un message cité, barre de défilement au survol et pendant le défilement, accusés de lecture (« Vu par » sous le dernier message lu de chacun) (`apps/estompe/natif/src/messagerie/defilement.rs`) ; pas encore : sélection de texte (copie par message entier) ; l'adresse `#/…/<message>` d'une notification ouvre la conversation au message (#342) |
| `components/MessageItem.svelte` | 2 | [#301](https://github.com/Plblsfr/plbls.fr/issues/301) | en cours | auteur, avatar, couleur du rôle, « Bot », badges, heure, regroupement, réponse citée, modifié, épinglé, système, mention, mise en évidence, markdown et mentions, masqués, copie du message, emoji en couleur, message d'emoji seuls en grand, pièces jointes, sondage, aperçu de lien, réactions (compte, la mienne, personnes au survol, temps réel), « Vu par » (cinq avatars et « +N », liste avec l'heure de lecture au clic), italique (`apps/estompe/natif/ui/messagerie/element.slint`, `annexes.slint`) ; barre d'actions au survol (trois réactions rapides, répondre, réagir, modifier, épingler, copier, supprimer, « Plus d'actions ») et menu complet (répondre, modifier, épingler et désépingler, ajouter une réaction, copier le texte, copier le lien, marquer comme non lu, supprimer avec confirmation, « Signaler » marqué « bientôt » jusqu'au lot 8), selon les droits du rôle ; clic ou Entrée sur une réaction pour l'ajouter ou la retirer, bouton d'ajout après les réactions (#318) ; écarts : « Marquer comme non lu » reste local à l'appareil (l'API n'a pas de route pour reculer la lecture), « Copier le lien » et « Signaler » n'existent pas dans le front ; manque la sélection d'une partie du texte (Slint) |
| `components/AttachmentView.svelte` | 2 | [#303](https://github.com/Plblsfr/plbls.fr/issues/303) | en cours | images (PNG, JPEG, GIF et WebP animés, en pause hors de l'écran), fichiers (téléchargement avec progression dans un dossier choisi, ouverture), fichier expiré ; replays et vidéos : carte « bientôt dans l'app native » jusqu'au lot 9 ; la taille d'une image n'est connue qu'après son premier chargement (l'API n'annonce pas ses dimensions) |
| `components/Lightbox.svelte` | 2 | [#303](https://github.com/Plblsfr/plbls.fr/issues/303) | en cours | visionneuse d'image : zoom, déplacement, navigation entre les images du message, copie, enregistrement (`ui/messagerie/visionneuse.slint`) ; une image animée y est fixe ; vidéos et lecteur YouTube au lot 9 |
| `components/VoiceMessage.svelte` | 3 | [#323](https://github.com/Plblsfr/plbls.fr/issues/323) | en cours | lecture et pause, forme d'onde avec la position (clic pour se placer), durée, vitesse 1×, 1,5×, 2× sans changer la hauteur, un seul message joué à la fois, point « non écouté » gardé dans les préférences, téléchargement, « illisible » et « fichier expiré » (`apps/estompe/natif/ui/messagerie/vocal.slint`, `apps/estompe/natif/src/messagerie/pont/vocal.rs`) ; Opus en Ogg ou WebM décodé en Rust, AAC en MP4 par ffmpeg s'il est présent ; reste la transcription (lot 9, [#303](https://github.com/Plblsfr/plbls.fr/issues/303)) ; écarts : pas d'enchaînement automatique sur le message vocal suivant, vitesse commune à tous les messages, position oubliée en passant à un autre message |
| `components/PollCard.svelte` | 2 | [#304](https://github.com/Plblsfr/plbls.fr/issues/304) | fait | question, choix, pourcentages, mon choix, gagnants, état clos (à l'échéance aussi), résultats en temps réel, suppression par le menu du message (#318) ; voter, changer et retirer son vote (affiché tout de suite, annulé en cas de refus), « Clore », corbeille avec confirmation, « Voir les votants » ([#319](https://github.com/Plblsfr/plbls.fr/issues/319), `apps/estompe/natif/src/messagerie/pont/sondages.rs`) |
| `components/ConvPanel.svelte` | 3 | [#274](https://github.com/Plblsfr/plbls.fr/issues/274) | en cours | onglet « Épinglés » de la marge : auteur, date, markdown, nombre de fichiers, « Voir dans la conversation » (charge le message s'il est hors de la fenêtre), épingles et désépingles en temps réel, rechargé à la reconnexion, « Désépingler » si le rôle le permet (#318) (`apps/estompe/natif/ui/messagerie/epingles.slint`) |
| `components/SearchModal.svelte` | 3 | [#321](https://github.com/Plblsfr/plbls.fr/issues/321) | en cours | recherche de messages partout, sur le serveur ou dans la conversation ouverte, tri, filtres proposés et complétés par Tab, extrait avec termes surlignés, résultats groupés par conversation, suite au défilement, saut au message (chargé autour), dernières recherches, aucun résultat et erreur affichés ; palette « aller à » (accueil, serveurs, sections, salons, conversations privées, préfixes `#` et `@`) au clavier par Ctrl+K (`apps/estompe/natif/ui/coquille/recherche.slint`) ; actions (préfixe `>`) « Nouveau message privé » et statuts (#322) ; restent les actions du vocal (lot 4), du profil et des paramètres (lot 8), « Créer ou rejoindre un serveur » et « Signaler un bug » |

### Messagerie, écriture

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `components/Composer.svelte` | 3 | [#314](https://github.com/Plblsfr/plbls.fr/issues/314) | en cours | champ sur plusieurs lignes qui grandit jusqu'à 220 px, Entrée envoie, Maj+Entrée va à la ligne, compteur au-delà de 3 500 caractères, bandeaux de réponse et de modification annulables par Échap, flèche haut pour modifier son dernier message, brouillon par conversation gardé entre deux lancements, lecture seule, hors ligne, envoi optimiste avec « Envoi… », « Non envoyé » et « Réessayer » (`apps/estompe/natif/ui/messagerie/saisie.slint`, `apps/estompe/natif/src/messagerie/saisie.rs`, `apps/estompe/crates/client/src/store/messages/envois.rs`) ; autocomplétion de `@` (personnes, `@everyone`, `@here`, rôles), `#` (salons) et `:` (smileys), conversion de `:nom:`, bouton du sélecteur d'emoji (#315) ; pièces jointes (#316) : bouton « Joindre un fichier », glisser-déposer sur la fenêtre, collage d'une image ou de fichiers, aperçu (vignette, nom, taille), envoi en flux avec progression, annulation, « Réessayer », limites de taille et de nombre vérifiées avant l'envoi, images réduites à 1 920 px (`apps/estompe/natif/src/messagerie/pieces.rs`, `apps/estompe/crates/client/src/televersement.rs`) ; vidéo HEVC ou AV1 convertie en H.264 avec progression et annulation (#317) ; bouton « Créer un sondage » (#319) ; bouton « GIF » (#320) ; message vocal (#323) ; correction orthographique absente (pas de correcteur dans le `TextInput` de Slint) |
| `components/EditAttachments.svelte` | 3 | [#316](https://github.com/Plblsfr/plbls.fr/issues/316) | en cours | pièces jointes du message dans le bandeau de modification : retrait, ajout (bouton, dépôt, collage), liste finale envoyée par `PATCH` (`apps/estompe/natif/src/messagerie/saisie.rs`) ; reste le réordonnancement par glisser |
| `components/PollModal.svelte` | 3 | [#319](https://github.com/Plblsfr/plbls.fr/issues/319) | fait | bouton « Créer un sondage » de la zone de saisie (permission `create_polls`), question, 2 à 10 choix, plusieurs réponses, clôture (durées, sans clôture, personnalisée), mêmes règles et limites que le front avec un message d'erreur (`apps/estompe/natif/ui/messagerie/creation-sondage.slint`, `apps/estompe/crates/client/src/sondage.rs`) ; écart : la date personnalisée se tape `JJ/MM/AAAA HH:MM` (pas de sélecteur de date en Slint) et les limites de longueur deviennent des erreurs (pas de `maxlength` dans `TextInput`) |
| `components/VoiceNote.svelte` | 3 | [#323](https://github.com/Plblsfr/plbls.fr/issues/323) | fait | bouton « Message vocal » à la place d'« Envoyer » quand le champ est vide, niveau en direct, durée, annuler, pause puis réécoute, envoi (en réponse si un message est cité), arrêt à 5 minutes, micro refusé ou absent signalé, « Réessayer » après un échec (`apps/estompe/natif/ui/messagerie/saisie.slint`, `apps/estompe/crates/client/src/message_vocal/`) ; fichier Ogg Opus à 32 kbit/s ; écarts : pas de geste « maintenir pour enregistrer » (propre au tactile), pas de transcription (lot 9) |
| `components/NewDMModal.svelte` | 3 | [#322](https://github.com/Plblsfr/plbls.fr/issues/322) | fait | fenêtre « Nouveau message privé » ouverte par le panneau ou la palette : pseudo avec pseudos connus proposés (conversations, membres des serveurs, présence), flèches et Entrée, conversation existante rouverte sans doublon, erreurs du serveur affichées (`apps/estompe/natif/ui/coquille/nouveau-prive.slint`) ; pas de conversation de groupe : ni l'API (`POST /dms` prend un seul pseudo) ni le front n'en ont |

### Vocal, caméra, partage d'écran

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `components/Quality.svelte` | 4 | [#329](https://github.com/Plblsfr/plbls.fr/issues/329) | fait | barres de qualité de chaque participant (tuiles) et de sa propre connexion (barre du vocal), `apps/estompe/natif/ui/vocal/commun.slint` |
| `components/VoiceTimer.svelte` | 4 | [#329](https://github.com/Plblsfr/plbls.fr/issues/329) | fait | durée de sa présence dans la barre du vocal (depuis `since` du serveur, sinon la connexion) ; durée du salon à droite de chaque salon vocal du panneau, depuis le plus ancien participant (`oldestSince`), rafraîchie chaque seconde (`apps/estompe/natif/src/coquille/projection.rs`) |
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
| `components/SettingsModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | en cours | onglet vocal dans « Réglages audio » ouvert depuis le vocal (#329, `apps/estompe/natif/ui/vocal/reglages.slint`) : micro et sortie, jauge, test du micro, bruit, gain, détection de voix et seuil, appuyer pour parler (touche, délai de relâchement, #330), volume général, sons d'arrivée et de départ avec le choix du son parmi ceux du front et leur volume (sans fichier personnalisé) ; restent compte, apparence, caméra, raccourcis et pages de la plateforme |
| `components/SettingsDebug.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | journal de débogage ; le journal lui-même vient de #291 |
| `components/ProfileModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | en cours | carte en lecture depuis la liste des membres (#297) : bannière, avatar et présence, pseudo de serveur et d'origine, badges, statut et jeu, exclusion, rôles, dates d'arrivée ; boutons « Envoyer un message » et « Modifier mon profil » marqués « bientôt » ; pas encore : modification des rôles, bannière, modération, historique, temps en vocal, statistiques de Sablier |
| `components/ServerSettingsModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | réglages d'un serveur |
| `components/RolesSettings.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/ChannelPerms.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | permissions d'un salon |
| `components/ChannelModModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | réglages et modération d'un salon |
| `components/IconCrop.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | recadrage de l'icône d'un serveur |
| `components/CreateChannelModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `components/CreateJoinModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | créer ou rejoindre un serveur |
| `components/ModerationModal.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | expulser, bannir, exclure, avertir, renommer |
| `components/AdminSpace.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | administration d'Estompe |
| `components/UpdateNotice.svelte` | 6 | [#345](https://github.com/Plblsfr/plbls.fr/issues/345) | fait | `apps/estompe/natif/ui/mise-a-jour/bandeau.slint` en bas du panneau de la coquille : notes de version (deux lignes puis « Plus »), « Mettre à jour » avec progression, « Plus tard » pour cette version, compte à rebours d'une mise à jour imposée ; Windows |
| `components/UpdaterPanel.svelte` | 6 | [#345](https://github.com/Plblsfr/plbls.fr/issues/345) | fait | `apps/estompe/natif/ui/mise-a-jour/panneau.slint` dans « À propos » (rangé dans les réglages au lot 8) : version installée, recherche, progression, « Installer et redémarrer », erreur, confirmation en appel ; sous Linux, lecture seule jusqu'au paquet (lot 11) |

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
| `blocks/activite-statut/` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | en cours | statut (en ligne, absent, ne pas déranger, hors ligne) et jeu en cours affichés dans la liste des membres, la carte et l'accueil, choix de son statut depuis la barre de l'utilisateur ou la palette (#322) ; détection et partage du jeu en cours au lot 7, « Écoute » avec la musique |
| `blocks/apercu-liens/` | 2 | [#304](https://github.com/Plblsfr/plbls.fr/issues/304) | en cours | appel `GET /link-preview` (`appels.rs`), cache par lien et message (200, LRU), préférence « aperçus » |
| `blocks/avatars/` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | en cours | avatars globaux et propres au serveur, demandés quand leur ligne devient visible, caches mémoire et disque bornés (`apps/estompe/natif/src/coquille/avatars.rs`), icônes des serveurs (#296) ; pas encore : images des badges, choix et recadrage de son avatar (lot 8) |
| `blocks/badges/` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | en cours | affichage dans les membres et la carte de profil (#297) ; messages au lot 2, administration au lot 8 |
| `blocks/draw2gether/` | 11 | [#283](https://github.com/Plblsfr/plbls.fr/issues/283) | absent |  |
| `blocks/emojis/` | 3 | [#275](https://github.com/Plblsfr/plbls.fr/issues/275) | en cours | lecture faite (#302) : emoji Unicode en images Twemoji embarquées, emoji de serveur (`GET /emojis`, `emoji.updated`) ; sélecteur, autocomplétion de `:` et conversion de `:nom:` à la saisie (#315, `apps/estompe/natif/src/messagerie/selecteur.rs`, `apps/estompe/crates/client/src/autocompletion.rs`) ; ajout d'une réaction depuis un message par le sélecteur (#318) ; reste la gestion des emoji de serveur (lot 8) |
| `blocks/gifs/` | 3 | [#320](https://github.com/Plblsfr/plbls.fr/issues/320) | fait | lecture au lot 2 (#303, GIF animés en pièce jointe) ; sélecteur (#320, `apps/estompe/natif/src/messagerie/gifs.rs`, `apps/estompe/crates/client/src/gifs.rs`) |
| `blocks/musique/` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent |  |
| `blocks/notifications/` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | en cours | notifications de l'OS faites (#342, `apps/estompe/crates/client/src/notifications.rs`), réglages au lot 8 ; push web : écart assumé ; écart sur le son : celui du système accompagne la notification, pas de bip quand les notifications du bureau sont coupées (la sortie du son ne sert encore qu'aux messages vocaux) |
| `blocks/regarder-ensemble/` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent | lecteur YouTube : écart assumé |
| `blocks/signalements/` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `blocks/transcription/` | 2 | [#303](https://github.com/Plblsfr/plbls.fr/issues/303) | absent | transcription des messages vocaux, sans composant propre |
| `blocks/videos/` | 9 | [#281](https://github.com/Plblsfr/plbls.fr/issues/281) | absent |  |
| `blocks/vocal/` | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | en cours | SDK Rust LiveKit (#324), périphériques audio (#325), session vocale (#326), chaîne du micro (#327), écoute (#328) dans `apps/estompe/crates/client/src/vocal/` et `audio/` ; interface du salon vocal (#329, `apps/estompe/natif/ui/vocal/`, `apps/estompe/natif/src/vocal/`) et appuyer pour parler par raccourci global (#330, `apps/estompe/crates/client/src/vocal/ptt.rs`) ; restent caméra, partage d'écran et scène vidéo (lot 5) |

### Composants des blocs

| Élément | Lot | Issue | État | Note |
|---|---|---|---|---|
| `blocks/apercu-liens/LinkCard.svelte` | 2 | [#304](https://github.com/Plblsfr/plbls.fr/issues/304) | en cours | carte avec et sans image, YouTube (miniature et titre, le clic ouvre le navigateur ; lecteur intégré au lot 9), carte Sillon sans pochette ni « Écouter » (section Sillon au lot 10) |
| `blocks/badges/BadgeIcon.svelte` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | en cours | médaille de la couleur du badge, nom en libellé accessible ; l'image (SVG, WebP, PNG, JPEG) et l'emoji du badge ne sont pas encore affichés |
| `blocks/badges/BadgeInline.svelte` | 1 | [#297](https://github.com/Plblsfr/plbls.fr/issues/297) | en cours | deux badges puis « +N » à côté du pseudo dans la liste des membres ; dans les messages, deux badges à côté du pseudo (#301) |
| `blocks/badges/BadgeRow.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | en cours | rangée de badges de la carte de profil (#297), sans l'infobulle du nom et de la description |
| `blocks/badges/BadgeAdmin.svelte` | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent |  |
| `blocks/draw2gether/Draw2Gether.svelte` | 11 | [#283](https://github.com/Plblsfr/plbls.fr/issues/283) | absent |  |
| `blocks/draw2gether/DrawCanvas.svelte` | 11 | [#283](https://github.com/Plblsfr/plbls.fr/issues/283) | absent |  |
| `blocks/draw2gether/DrawEnd.svelte` | 11 | [#283](https://github.com/Plblsfr/plbls.fr/issues/283) | absent |  |
| `blocks/draw2gether/DrawLobby.svelte` | 11 | [#283](https://github.com/Plblsfr/plbls.fr/issues/283) | absent |  |
| `blocks/draw2gether/DrawPlay.svelte` | 11 | [#283](https://github.com/Plblsfr/plbls.fr/issues/283) | absent |  |
| `blocks/emojis/EmojiPicker.svelte` | 3 | [#315](https://github.com/Plblsfr/plbls.fr/issues/315) | fait | recherche (nom, code, mot-clé), neuf catégories et récents (gardés dans les préférences), emoji de serveur, nom et code en pied, Maj+Entrée ou Maj+clic pour en insérer plusieurs, tout au clavier (flèches, Tab vers les catégories, Échap), cible saisie ou réaction (`apps/estompe/natif/ui/messagerie/selecteur.slint`) ; pas de teinte de peau, comme le front |
| `blocks/gifs/GifPicker.svelte` | 3 | [#320](https://github.com/Plblsfr/plbls.fr/issues/320) | écart assumé | tendances, recherche après 350 ms sans frappe, « Tendances » et idées, grille animée (seuls les GIF à l'écran), suite au défilement, envoi par `POST /gifs/import` (en réponse si un message est cité), attribution GIPHY, messages 501, 429 (sans nouvelle tentative) et autres, tout au clavier (`apps/estompe/natif/ui/messagerie/gifs.slint`) ; écarts : rangées justifiées au lieu de la maçonnerie, indicateur de chargement au lieu des cases grisées |
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
| `blocks/vocal/VoiceChannelView.svelte` | 4 | [#329](https://github.com/Plblsfr/plbls.fr/issues/329) | fait | écran pour rejoindre (participants, compte, erreur, permission), scène en grille de 1 à 12 tuiles et plus, « tu es seul·e ici », bandeau de reconnexion, commandes en bas (`apps/estompe/natif/ui/vocal/vue.slint`), mode plein cadre (rail, panneau, en-tête et marge masqués, plein écran, Échap pour sortir) ; Regarder et Écouter ensemble, Draw2Gether et la tuile agrandie d'un flux vidéo suivent leurs lots (5 et 9) |
| `blocks/vocal/VoiceControls.svelte` | 4 | [#329](https://github.com/Plblsfr/plbls.fr/issues/329) | en cours | micro avec jauge, casque, choix du micro et de la sortie, réglages audio, raccrocher, raccourcis Alt+Maj+M et Alt+Maj+D, « Ton micro est coupé », rappel de la touche d'appuyer pour parler (`apps/estompe/natif/ui/vocal/commandes.slint`) ; caméra et partage visibles et marqués « bientôt » (lot 5) ; pas d'overlay des voix (lot 7) |
| `blocks/vocal/StageTile.svelte` | 4 | [#329](https://github.com/Plblsfr/plbls.fr/issues/329) | fait | tuile d'un participant : photo de profil (sinon initiales), pseudo, « (toi) », halo et onde quand il parle, micro et casque coupés, coupures du serveur, qualité ; menu au clic ou au clavier : muet pour moi, volume de 0 à 300 %, coupures du serveur pour qui en a le droit, « Déplacer vers » un autre salon vocal et « Déconnecter du vocal » (`move_members`, `disconnect_members`, comme le menu du panneau du front) (`apps/estompe/natif/ui/vocal/tuile.slint`, `apps/estompe/natif/ui/vocal/menus.slint`) ; le volume du son d'un partage et les flux caméra et écran suivent le lot 5 |

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
| Conversion d'une vidéo en H.264 avant l'envoi (ffmpeg embarqué) | 3 | [#317](https://github.com/Plblsfr/plbls.fr/issues/317) | en cours | même règle que `apps/estompe/windows/src-tauri/src/video.rs` (MP4 hors H.264, encodeurs matériels, original sinon), progression, annulation, fichier temporaire supprimé, même code sous Linux et Windows (`apps/estompe/crates/client/src/video.rs`) ; ffmpeg pris à côté de l'exécutable ou dans le `PATH` : reste à l'embarquer dans l'installeur natif (#278, environ 40 Mo compressé) |
| Partage d'écran natif : liste des écrans et fenêtres avec miniatures | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | absent | `apps/estompe/windows/src/capture.ts`, `apps/estompe/windows/src-tauri/src/capture.rs` |
| Partage d'écran natif : capture d'une fenêtre | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | absent | `apps/estompe/windows/src-tauri/src/capture_fenetre.rs` |
| Partage d'écran natif : publication dans le salon par Rust (LiveKit) | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | absent | `apps/estompe/windows/src-tauri/src/screen_native.rs` |
| Son du partage (fenêtre, ou système sans Estompe) par WASAPI | 5 | [#277](https://github.com/Plblsfr/plbls.fr/issues/277) | absent | `apps/estompe/windows/src-tauri/src/son_wasapi.rs` |
| Installeur NSIS | 6 | [#343](https://github.com/Plblsfr/plbls.fr/issues/343) | fait | `apps/estompe/natif/installeur/estompe-natif.nsi` (par utilisateur, raccourcis, identité des toasts, désinstallation qui garde les données), tag `estompe-natif-v*` ; non signé Authenticode (SmartScreen) ; pas encore : embarquer `ffmpeg.exe` à côté d'`estompe-natif.exe` (`apps/estompe/windows/scripts/fetch-ffmpeg.mjs`, 130 Mo installé) pour la conversion des vidéos ; app Tauri : `apps/estompe/windows/scripts/release-windows.sh`, `apps/estompe/windows/scripts/check-installer.sh` |
| Mises à jour signées, installation et redémarrage | 6 | [#344](https://github.com/Plblsfr/plbls.fr/issues/344) | fait | `apps/estompe/crates/client/src/mise_a_jour/`, interface `apps/estompe/natif/ui/mise-a-jour/` (#345) (manifeste `natif.json`, hôtes de GitHub en HTTPS, reprise, signature minisign et version vérifiées avant le lancement, installation silencieuse `/S /RELANCER` puis redémarrage, vérification au lancement puis toutes les 30 minutes) ; Windows, Linux au lot 11 ; app Tauri : `apps/estompe/windows/src/updates.ts` |
| Instance unique : un second lancement ramène la fenêtre | 6 | [#341](https://github.com/Plblsfr/plbls.fr/issues/341) | fait | `apps/estompe/windows/src-tauri/src/lib.rs` ; socket local ou tube nommé propre à `fr.plbls.estompe.natif`, arguments transmis (`apps/estompe/crates/plateforme/src/instance.rs`) |
| Icône de la zone de notification ; la fenêtre se masque à la fermeture | 6 | [#341](https://github.com/Plblsfr/plbls.fr/issues/341) | fait | `apps/estompe/windows/src-tauri/src/lib.rs` ; clic, « Ouvrir », « Quitter » ; sous Linux sans zone de notification, fermer quitte (`apps/estompe/natif/src/bureau/mod.rs`) |
| Démarrage avec la session (ajout de l'app native, absent de l'app Tauri) | 6 | [#341](https://github.com/Plblsfr/plbls.fr/issues/341) | fait | Windows (valeur `EstompeNatif` de la clé `Run`), désactivé par défaut ; Linux au lot 11 |
| Toasts natifs avec « Répondre » et « Marquer comme lu » | 6 | [#342](https://github.com/Plblsfr/plbls.fr/issues/342) | fait | `apps/estompe/windows/src/notifications.ts`, `apps/estompe/windows/src-tauri/src/toast.rs` ; toasts WinRT (`apps/estompe/crates/plateforme/src/toast_windows.rs`) ; Linux : `notify-rust`, clic et « Marquer comme lu », sans réponse en ligne |
| Pastille de mentions sur l'icône de la barre des tâches | 6 | [#342](https://github.com/Plblsfr/plbls.fr/issues/342) | fait | `apps/estompe/windows/src/notifications.ts` ; aussi sur l'icône de la zone de notification ; Linux : pastille sur l'icône de la zone seulement, barre des tâches au lot 11 |
| Appuyer pour parler global, hors focus | 4 | [#330](https://github.com/Plblsfr/plbls.fr/issues/330) | en cours | trait `RaccourcisGlobaux` (appui et relâchement, touches seules comme l'app Tauri), crochet clavier bas niveau sous Windows (la touche continue de parvenir au jeu), XInput 2 sous X11, indisponible sous Wayland (la touche marche fenêtre active, le réglage le dit), délai de relâchement (`apps/estompe/crates/plateforme/src/raccourcis/`) ; essai réel dans un jeu sous Windows non fait |
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
| Mesures : relevés de l'app et du système | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | en cours | `apps/estompe/windows/src/mesures.ts`, `apps/estompe/windows/src/mesures.svelte.ts`, `apps/estompe/windows/src/SettingsMesures.svelte`, `apps/estompe/windows/src-tauri/src/mesures.rs`, `apps/estompe/windows/scripts/synthese-mesures.mjs` ; mémoire et CPU du processus pendant un scénario, lancé par `--mesures`, export au même format lu par `synthese-mesures.mjs` (#331, `apps/estompe/natif/src/mesures.rs`, `apps/estompe/crates/client/src/mesures.rs`) ; restent l'écran de mesure, les scénarios guidés, la fluidité des images, les latences et le démarrage |
| Options de WebView2 (économie de mémoire, purge des données à chaque version) | 7 | [#279](https://github.com/Plblsfr/plbls.fr/issues/279) | absent | `apps/estompe/windows/src-tauri/src/webview2_options.rs`, `apps/estompe/windows/src-tauri/src/webview_data.rs` ; propre à la webview, à trancher au lot 7 |
| Capture de la fenêtre d'Estompe pour un signalement | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | `apps/estompe/windows/src-tauri/src/capture.rs` |
| Déblocage des fonctions alpha (7 clics sur la version) | 8 | [#280](https://github.com/Plblsfr/plbls.fr/issues/280) | absent | `apps/estompe/windows/src/UpdateSettings.svelte` |
