# App native (Slint, sans webview)

« Estompe natif (beta) » est l'app de bureau écrite entièrement en Rust, sans webview, à côté de l'app Tauri qui reste la version stable.
Elle s'installe et tourne en même temps qu'elle. Ce qui lui manque encore est dans la [matrice de parité](natif-parite.md).
En place : la fenêtre, l'écran « À propos », la page de démonstration des composants, les écrans d'entrée (connexion, accès refusé, activation, invitation, mise à jour forcée) la coquille (rail des serveurs, salons, messages privés, accueil, navigation), la marge « Membres » (liste des membres, présence, activité, carte de profil) et la messagerie en lecture (messages des salons texte et des conversations privées, emoji, pièces jointes, aperçus de liens, sondages, réactions, temps réel, marquage comme lu) et le début de l'écriture (zone de saisie, envoi, modification, brouillons, mentions et emoji, pièces jointes, conversion d'une vidéo en H.264, actions sur un message et menus contextuels, sondages, GIF, messages vocaux, voir [Écriture](#écriture)), la recherche de messages et la palette (voir [Recherche et palette](#recherche-et-palette)), la nouvelle conversation privée, « Tout marquer lu », les contacts en ligne et le menu de statut (voir [Nouvelle conversation, accueil et statut](#nouvelle-conversation-accueil-et-statut)), la création d'un serveur ou d'un salon, rejoindre et quitter un serveur (voir [Créer ou rejoindre un serveur, créer un salon](#créer-ou-rejoindre-un-serveur-créer-un-salon)), les réglages d'un serveur, ses rôles et les permissions des salons (voir [Réglages d'un serveur](#réglages-dun-serveur) et [Rôles et permissions](#rôles-et-permissions)), la modération des membres et des salons (voir [Modération](#modération)), l'intégration au bureau (instance unique, zone de notification, voir [Intégration au bureau](#intégration-au-bureau) ; notifications du système et pastille de mentions, voir [Notifications](#notifications)), les mises à jour signées avec leur interface (voir [Mises à jour](#mises-à-jour)), la détection du jeu au premier plan et l'activité montrée aux autres sous Windows (voir [Jeu au premier plan](#jeu-au-premier-plan)), la page des paramètres (voir [Paramètres](#paramètres)) ; le reste de l'écriture arrive avec le lot 3. Le vocal est en place : salon vocal, commandes, réglages audio et appuyer pour parler par raccourci global (voir [Vocal](#vocal)), ainsi que l'affichage des flux vidéo, la scène vidéo avec le visionnage des partages d'écran des autres, la caméra et son propre partage d'écran (voir [Vidéo](#vidéo), [Scène vidéo et partages](#scène-vidéo-et-partages), [Caméra](#caméra) et [Partage d'écran](#partage-décran)).

## Architecture

| Dossier | Rôle |
|---|---|
| `apps/estompe/natif` | binaire `estompe-natif` : interface Slint (`ui/`), moteur de rendu Skia (logiciel pour les overlays, `src/overlay/`), branchement des préférences et du journal (`src/session.rs`), écrans d'entrée (`src/entree.rs`, `src/ecrans.rs`), coquille (`src/coquille/`), intégration au bureau (`src/bureau/`) |
| `apps/estompe/crates/client` | logique sans interface : types de l'API générés depuis `openapi.yaml`, client HTTP, adresse du serveur, préférences, journal, store, temps réel, routes, accès aux sections, vocal (`vocal/`), audio (`audio/`), messages vocaux (`message_vocal/`) et mesures (`mesures.rs`) |
| `apps/estompe/crates/plateforme` | tout le code propre à un OS, derrière des traits ; dossiers de l'utilisateur ; périphériques audio (`audio.rs`) ; caméras (`camera.rs`) ; compteurs du processus pour les mesures (`processus/`) ; capture d'écran et de fenêtre (`capture/`) |

Les trois paquets forment un workspace Cargo à part (`apps/estompe/natif/Cargo.toml`, un `Cargo.lock`, toolchain stable, `deny.toml` propre),
hors du workspace racine comme les apps Tauri. Slint est figé en `=1.18.1`, comme dans l'overlay natif de l'app Tauri.
L'arbre de dépendances ne contient ni `tauri`, ni `wry`, ni `webview2-com` : `deny.toml` les interdit.

## Construire et lancer

Prérequis sous Linux (Debian, Ubuntu) : `libfontconfig1-dev libfreetype-dev libxkbcommon-x11-0 libasound2-dev libglib2.0-dev`, libclang (les en-têtes V4L2 de la capture de la caméra passent par bindgen), un pilote OpenGL (Mesa)
et clang 21 ou plus récent, désigné par `CXX` (par exemple `CXX=clang++-21`) : libwebrtc du SDK LiveKit est compilé contre la libc++ de Chromium,
que GCC et les clang plus anciens ne savent pas utiliser (le build de `webrtc-sys` le refuse). Sous Windows : la toolchain MSVC, LLVM, Python 3 et
`ninja`, LLVM 22 ou plus récent (voir [Vocal](#vocal), « CRT statique sous Windows »). Les binaires de Skia (Linux) et de libwebrtc sont téléchargés à la première compilation.
`cargo build --no-default-features` construit l'app sans le SDK LiveKit (vocal indisponible). ffmpeg, s'il est présent, lit les messages vocaux en AAC.

```sh
just estompe-natif-run            # build de débogage : le bouton « Composants » ouvre la page de démonstration
just estompe-natif-build          # profil release (taille, LTO) ; `just estompe-natif-build ci` pour le profil sans LTO
just estompe-natif-test
just estompe-natif-check          # tous les contrôles de la CI Linux
```

`estompe-natif --essai-de-lancement` ouvre la fenêtre, attend sa première image puis quitte : c'est le lancement fait par la CI sous Xvfb.
`estompe-natif --mesures <fichier.json>` relève la mémoire et le CPU de l'app pendant un scénario et écrit un export au format de l'app Tauri :
voir [natif-mesures.md](natif-mesures.md) (mesure intégrée, protocole et résultats du point de contrôle du lot 4).

`estompe-natif --arriere-plan` démarre sans montrer la fenêtre si l'icône de la zone de notification est en place (lancement à l'ouverture de la session), sinon la montre.

## Interface

- `ui/genere/couleurs.slint` et `ui/genere/icones.slint` sont générés par `scripts/estompe/natif-da.mjs` depuis
  `web/estompe/src/styles/v3-jetons.css` (thèmes « Papier » et « Fusain ») et `web/estompe/src/components/Icon.svelte`. Après un changement de
  l'un ou de l'autre : `just estompe-natif-generer`. La CI échoue si les fichiers versionnés ne correspondent plus (`--check`).
- `ui/jetons.slint` : typographie, rayons, espacements et durées de [la direction artistique](direction-artistique.md) ; les trois polices
  variables (`polices/`, licence OFL) sont embarquées dans le binaire. Tout texte de l'app utilise l'une d'elles (le code en ligne compris, en
  JetBrains Mono) ; seuls les caractères absents des trois polices (écritures non latines, emoji hors du corps d'un message : pseudo, citation de réponse) passent par une police du système.
- `emoji/twemoji.bin` : les emoji en couleur (voir [Emoji](#emoji)), licences `emoji/CC-BY-4.0-twemoji.txt` et `emoji/MIT-emoji-data.txt`, citées dans « À propos ».
- `ui/jetons.slint` porte aussi `Affichage.compacte` (densité des messages), réglée par les [paramètres](#paramètres).
- `ui/composants/` : texte riche (`TexteRiche`, blocs de markdown), séparateur de date ou de non-lus (`Separateur`), bouton, champ de saisie, case, interrupteur, modal, infobulle, icône, avatar et pastille de présence, recadrage d'image (`Recadrage`), indicateur de chargement, zone défilante,
  liste virtualisée (`ListeVirtuelle`), compteur (pastille de non-lus ou de mentions), étiquette (« bientôt »), badge de rôle (« Bot », « Créateur »), élément de liste, état vide, coins arrondis d'une image,
  menu contextuel et confirmation (voir [Menus contextuels et confirmation](#menus-contextuels-et-confirmation)).
- Un composant interactif placé dans un modal reçoit `calque: 1` : tant qu'un modal est ouvert, ceux du dessous n'acceptent ni focus ni clic.
- L'écran « À propos » affiche la version, l'attribution exigée par la licence royalty-free de Slint (`AboutSlint`) et celle de FFmpeg (LGPL, voir [Pièces jointes à l'envoi](#pièces-jointes-à-lenvoi)).

### Menus contextuels et confirmation

- Menu (`ui/composants/menu.slint`, global `DonneesMenu` ; `src/menu.rs`) : une seule instance, au-dessus de tout dans `ui/app.slint`. `menu::ouvrir(f, titre, ancre, entrées, action)`
  le remplit et le place : au curseur (`Ancre::Point`) ou sous un bouton (`Ancre::Element`, retourné au-dessus s'il manque de place en bas, `retourner`), toujours gardé dans la
  fenêtre (`placer`, marge de 8 px), comme `placeMenu` et `flipY` du front ; la hauteur est calculée en Rust (`hauteur`). Sans entrée, rien ne s'ouvre. Une entrée (`Entree`) a
  une icône et peut être séparée, dangereuse, cochée, porter un raccourci ou être marquée « bientôt » (affichée, sautée par les flèches, inactive).
- Clavier : flèches (en boucle), Tab, Origine, Fin, Entrée ou Espace, Échap. Un clic hors du menu, la molette ou la perte du focus le ferment. L'élément qui l'a ouvert retient
  `a-ouvert-menu` et reprend le focus à la fermeture, sauf si l'action l'a donné ailleurs (l'action rend `true` : champ de saisie, sélecteur, confirmation ; `focus-ailleurs`).
  Une action qui ouvre un autre menu (durées d'une sourdine) le remplace : `menu::choisir` ne referme pas un menu rouvert par l'action.
- Ouverture : clic droit, touche Menu ou Maj+F10 sur l'élément qui a le focus. Un `FocusScope` qui prend le focus au clic avale l'appui : la ligne d'un message a
  `focus-on-click: false` et se donne le focus dans `pointer-event`, sinon le premier clic droit ne ferait que la sélectionner.
- Une `TouchArea` imbriquée prend tout appui, clic droit compris, et la zone parente ne le reçoit plus : l'avatar et le nom de l'auteur d'un message relaient le clic droit
  au menu du message (`clic-droit` de `CorpsMessage`), comme le clic droit du front remonte jusqu'à l'article. Où le clic droit ouvre un menu sur un utilisateur :
  liste des membres (menu ci-dessus ; le clic ouvre la carte de profil), message (menu du message), conversation privée du panneau (« Copier le pseudo » avec
  « Marquer comme lu » et « Copier le lien »), participant d'un vocal dans le panneau et tuile du salon (menu du participant, voir [Interface du salon vocal](#interface-du-salon-vocal)).
- Confirmation (`ui/composants/confirmation.slint`, global `DonneesConfirmation` ; `src/confirmation.rs`) : `confirmation::confirmer` et `confirmation::saisir`, sur le modal
  commun : titre, message, bouton d'action (rouge si destructive), saisie éventuelle (à recopier exactement : `exiger`) ou case à cocher ; `peut_valider` décide si le bouton est actif
  (le rappel pur reçoit la demande en argument pour que Slint le réévalue à chaque nouvelle demande). Une nouvelle demande annule la précédente (sa suite reçoit un refus), comme `confirm.svelte.ts`.
- Copie d'un texte depuis Rust : `menu::copier_texte` passe par le `TextInput` caché de `CopieTexte` (global `DonneesCopie`).

### Jetons

| CSS (`v3-jetons.css`) | Slint |
|---|---|
| couleurs `--papier`, `--encre`, `--rose-fonce`, `--av-1-t`… des deux thèmes | `Theme.papier`, `Theme.encre`, `Theme.rose-fonce`, `Theme.av-1-t`… (même nom sans `--`), choisis par `Theme.mode` (`auto` suit le système) |
| `--calque` | `Theme.calque` |
| `--f-titre`, `--f-texte`, `--f-mono` | `Typo.titre`, `Typo.texte`, `Typo.mono` |
| corps à 15 px, 14 px en densité compacte, 13 px | `Typo.corps`, `Typo.compact`, `Typo.petit` |
| `--r-xs`, `--r-s`, `--r`, `--r-l`, `--r-xl`, `--r-p` | `Rayon.xs`, `Rayon.s`, `Rayon.m`, `Rayon.l`, `Rayon.xl`, `Rayon.pilule` |
| `--espace-1` … `--espace-16` | `Espace.e1` … `Espace.e16` |
| `--d`, `--d-l`, `--sortie`, `--ressort` | `Mouvement.d`, `Mouvement.d-l`, `Courbes.sortie`, `Courbes.ressort` |
| `data-motion="reduit"` | `Mouvement.reduit` (durées à zéro) |

### Écarts de rendu avec le front web

Même direction artistique, pas au pixel :
- les élévations `--o1`, `--o2`, `--o3` (plusieurs ombres superposées) deviennent un contour et une seule ombre portée ;
- le halo de focus `--halo` et l'anneau `--anneau` deviennent un anneau de 2 px d'encre, décalé de 2 px, affiché pour un focus venu du clavier ;
- la barre de défilement apparaît au survol de la zone, sans le fondu de `scrollbar-color` ;
- anticrénelage et crénage du texte sont ceux de Skia, pas ceux du navigateur ;
- la position de la fenêtre est retenue telle quelle : si l'écran où elle se trouvait n'existe plus, elle peut s'ouvrir hors de l'écran.

## Écrans d'entrée et authentification

`src/entree.rs` décide de l'écran affiché, sans interface : une machine d'états (`Machine::traiter(Evenement) -> Option<Effet>`, testée seule)
et une boucle (`Entree`) qui exécute ses effets sur un runtime tokio à un fil, sur un fil dédié (`estompe-entree`).

| État (`Etat`) | Écran (`Page` de `ui/app.slint`) |
|---|---|
| `Demarrage` : reprise de la session depuis le coffre | `demarrage` |
| `Deconnecte`, `AttenteNavigateur`, `Erreur(message)` (délai dépassé, navigateur indisponible, refus, réseau, session expirée) | `connexion` |
| `Verification` : `GET /me` | `verification` |
| `Refuse` (`allowed: false`), `AActiver` (`activated: false`, `POST /activate`) | `refuse`, `activation` |
| `Invitation` (`POST /server-invites/{code}/join`) | `invitation` |
| `Injoignable(message)` : `/me` ou la reprise ont échoué sans refus | `injoignable` |
| `Connecte { compte, serveur }` | `coquille` |
| `MiseAJourRequise { message, version_minimale }` | `mise-a-jour` |

- L'authentification et l'API sont injectées (traits `Authentifieur` et `ApiEntree`) : `AuthReelle` (crate client, `Authentification`) et
  `Client` en production, des faux dans `tests/commun/`. `src/ecrans.rs` traduit un `Etat` en propriétés de la fenêtre (`afficher`) et les
  rappels de la fenêtre en `Evenement` (`brancher`) ; l'état revient à l'interface par `upgrade_in_event_loop`.
- `lib.rs` (`demarrer`) assemble le tout et rend `Application { entree, client, auth }` : la coquille et le store se branchent sur ce client
  (jeton, plateforme et version déjà posés), sur `entree.runtime()` pour leurs tâches et sur `entree.emetteur()` pour revenir à l'entrée (déconnexion, mise à jour).
- Connexion : PKCE dans le navigateur du système et redirection sur `127.0.0.1` (crate client, `auth`), délai de 5 minutes, annulable depuis
  l'écran ; le refresh token est dans le coffre de secrets et la session reprend au lancement. Émetteur Keycloak : `https://auth.plbls.fr/realms/default`,
  ou `ESTOMPE_KEYCLOAK_ISSUER` pour un Keycloak de test (une adresse invalide est ignorée).
- Invitation : `estompe-natif --invitation <lien ou code>` la rejoint dès le compte vérifié (écran `invitation`) ; depuis la coquille, le « + » du rail ouvre
  la fenêtre « Créer ou rejoindre un serveur » (voir [Créer ou rejoindre un serveur, créer un salon](#créer-ou-rejoindre-un-serveur-créer-un-salon)). Le code est pris à la fin d'un lien (`ABCDE-23456`). Les liens `estompe://` ouverts par le système ne sont pas encore pris en charge.

### Mise à jour forcée

Le client déclare `X-Estompe-Platform: natif` et `X-Estompe-Version: <version du paquet>` ; l'API applique `platforms.natif` de
`api/estompe/core/updates.json`. Un refus 426 (`upgrade_required`), à n'importe quel appel, mène à l'écran bloquant « Mise à jour requise »
(message du serveur, version installée et minimale) : la machine n'émet plus aucune requête vers l'API. Une mise à jour imposée par le manifeste
(`mandatory`, `minVersion`, voir [Mises à jour](#mises-à-jour)) y mène aussi (`Evenement::MiseAJourSignalee`), après un compte à rebours de 120 s
si l'utilisateur écrit.

- Installation disponible (Windows, `Machine::avec_installation`) : « Installer la mise à jour » (`Evenement::Telecharger` → `Effet::Installer`)
  télécharge, vérifie et lance l'installeur depuis l'écran (progression, puis redémarrage) ; en cas d'échec, l'erreur s'affiche avec « Réessayer » et
  « Ouvrir la page de téléchargement » (`Evenement::OuvrirPageTelechargement`).
- Sinon (Linux) : le bouton ouvre dans le navigateur la page du serveur (`ESTOMPE_API_URL` ou `https://chat.plbls.fr`).

## Coquille

Une fois l'entrée à `Connecte`, la coquille remplace les écrans d'entrée : rail des serveurs, panneau de navigation, zone de contenu et,
à droite, la marge.

| Élément | Interface (`ui/coquille/`) | Contenu |
|---|---|---|
| Rail | `rail.slint` | accueil (non-lus et mentions des messages privés), serveurs dans l'ordre choisi (image du serveur, sinon initiales ; trait de non-lus, pastille de mentions), « Créer ou rejoindre un serveur », entrées des sections ouvertes |
| Panneau | `panneau.slint` | salons du serveur courant (texte, esquisses, vocaux, liste simple comme `SidePanel.svelte`, « + » de création avec le droit de gérer les salons), ou accueil et messages privés (bouton « Nouveau message privé »), ou section ; barre du compte (statut ou jeu, menu de statut) avec « Se déconnecter » ; repliable |
| Contenu | `contenu.slint`, `accueil.slint`, `bandeau.slint` | en-tête (nom du salon, sujet) ; accueil (salut, à rattraper avec « Tout marquer lu », vocaux en cours, contacts en ligne) ; état vide qui annonce le lot du contenu ; bandeau « Reconnexion en cours… » ou hors ligne |
| Marge | `marge.slint`, `membres.slint`, `carte-profil.slint`, `../messagerie/epingles.slint` | `EmplacementMarge` ; onglets « Membres » sur un serveur (voir [Membres](#membres)) et « Épinglés » dans une conversation (voir [Épinglés](#épinglés)), carte de profil |

- Données : global `DonneesCoquille` (`ui/coquille/donnees.slint`) : modèles des lignes (`serveurs`, `salons`, `prives`, `sections`, `rattrapage`,
  `vocaux`), en-tête du panneau, vue, accueil, bandeau, et les rappels de navigation (`ouvrir-serveur`, `ouvrir-salon`, `deplacer-serveur`…).
- `src/coquille/projection.rs` : fonctions pures, état du store et route vers les lignes (`lignes_serveurs`, `lignes_salons`, `donnees_vue`…), testées sans interface.
  `navigation.rs` : route courante, dernier salon vu par serveur, retour à l'accueil si le serveur ou le message privé disparaît, route de démarrage depuis la dernière vue.
- `modeles.rs` : mises à jour fines. `cibles` traduit les `Changement` du store en lignes visées (un non-lu ne vise que la ligne de son serveur et
  celle de son salon), `synchroniser` compare une liste à son ombre et ne retire, insère ou remplace que les lignes qui diffèrent : un `VecModel` n'est jamais reconstruit pour un élément modifié.
- `pont.rs` (`Pont`) : applique les projections aux modèles sur le fil de l'interface, télécharge les icônes des serveurs (`GET /servers/{id}/icon`, PNG ou JPEG, réduites à 128 px), retient la dernière vue, l'ordre des serveurs et le panneau replié dans les préférences.
- `cycle.rs` (`Coquille`) : suit l'état de l'entrée sur son runtime. À `Connecte`, la fabrique (`FabriqueReelle` : store, temps réel, accès aux sections ;
  un faux dans les tests) ouvre la session : le store se charge (nouvel essai avec attente croissante si le réseau manque), les changements partent
  vers l'interface par `upgrade_in_event_loop`. Hors de `Connecte` et de `Invitation`, la session s'arrête et les modèles se vident. `FinFlux::SessionExpiree`
  ou un 401 mènent à `Evenement::SessionPerdue` (écran de connexion, « Ta session a expiré »), `FinFlux::AccesRetire` à `Evenement::AccesPerdu` (nouvelle vérification du compte).
- Navigation : clic sur un serveur (son dernier salon vu, sinon le premier salon texte), un salon, un message privé, une entrée d'« À rattraper » ;
  Ctrl+Alt+↑/↓ passe au serveur voisin ; un serveur se déplace en le glissant sur le rail. Au lancement, la dernière vue (`derniere_vue` des préférences, propre au compte) est rouverte.
- Sections : Entracte, Sillon, Stats (Sablier) et Régie apparaissent sur le rail si le `GET /me` de leur service les ouvre (`permissions.access`,
  `permissions.admin` pour Régie), comme le front. Adresses : `ESTOMPE_ENTRACTE_URL`, `ESTOMPE_SILLON_URL`, `ESTOMPE_SABLIER_URL`, `ESTOMPE_REGIE_URL`
  (vide : section coupée) ; sans variable, celles de production pour le serveur de production (Régie n'en a pas). Leur contenu arrive au lot 10.
- Salons vidéos et musique sont listés avec l'étiquette « bientôt » ; leur vue l'explique. Un salon vocal ou cinéma ouvre la vue du vocal et le rejoint (voir [Interface du salon vocal](#interface-du-salon-vocal)). Un salon texte ou un message privé ouvre la vue des messages (voir [Messagerie](#messagerie)).
- Menu contextuel d'un salon texte ou d'une conversation privée du panneau (`src/coquille/pont/menus.rs`) : « Marquer comme lu » s'il y a des non-lus ou des mentions
  (`Store::lire_conversation`), « Copier le lien » (`https://<serveur>/#/s/…` ou `#/dm/…`) ; un salon « bientôt » n'a que le lien.
- Messages privés du panneau : triés par activité (`lignes_privees` du store, une conversation qui reçoit un message remonte), avec non-lus, mentions et présence par formes sur l'avatar.

### Membres

Sur un serveur, `DonneesCoquille.marge-disponible` passe à vrai : le bouton « Membres » de l'en-tête et Ctrl+. ouvrent ou ferment la marge, le
bouton « Fermer la marge » la ferme ; l'ouverture est retenue dans les préférences (`marge_ouverte`, ouverte par défaut comme le front). Hors d'un serveur, il n'y a pas de marge.

- Interface : `MargeMembres` (`ui/coquille/membres.slint`), enfant de `Coquille` dans `ui/app.slint`, instancié seulement quand la marge est ouverte.
  En tête, l'onglet « Membres », le compteur `MEMBRES · en ligne/total` et, au-delà de 8 membres, le filtre (pseudo et pseudo de serveur, sans casse ni accents).
- Liste : groupes des rôles affichés séparément (`hoist`, du plus haut au plus bas, comme `grouper_membres`), puis « EN LIGNE » et « HORS LIGNE » pour les autres ;
  dans chaque groupe, en ligne d'abord puis par pseudo. Une ligne : avatar avec pastille de présence, pseudo de serveur à la couleur du rôle (atténué hors ligne),
  cadenas d'exclusion, « Bot », « Créateur », deux badges puis « +N » (médaille à la couleur du badge), jeu en cours. Un clic ou Entrée ouvre la carte de profil ; clic droit, touche Menu ou Maj+F10 ouvrent le menu du membre : « Profil », « Mentionner »
  (si une conversation de ce serveur est ouverte en écriture : `@pseudo ` inséré au curseur), « Copier le pseudo ».
- Présence par formes, lisible sans couleur (`Pastille`, `ui/composants/avatar.slint`) : disque plein (en ligne), demi-lune (absent), disque barré (ne pas déranger), anneau (hors ligne).
- Virtualisation : la liste est une `ListeVirtuelle`, réimplémentation de `ListView` aux couleurs de la DA (le compilateur Slint ne virtualise que les enfants d'un
  composant nommé `ListView`) ; toutes les lignes font 44 px, seules les lignes visibles sont instanciées (16 pour 5 000 membres dans une fenêtre de 900 px).
  La liste signale son défilement et sa hauteur (`membres-defiles`) ; `fenetre_des_membres` (`src/coquille/membres.rs`) en déduit les lignes visibles, plus 5 lignes au-dessus et au-dessous.
- Projection pure : `lignes_membres` (état du store, serveur, filtre → lignes, en-têtes de groupe compris), `donnees_marge`, `donnees_carte`. Les `Changement`
  `MembreAjoute`, `MembreRetire`, `Membre`, `Roles`, `Presence` et `Avatar` du serveur courant visent la liste (`Cibles::membres`) ; `synchroniser` n'en touche
  que les lignes qui diffèrent : un changement de statut remplace une ligne, un passage en ligne retire et insère la seule ligne du membre et met à jour les deux compteurs.
- Carte de profil (`ui/coquille/carte-profil.slint`), en lecture : bannière (`banner_color`, sinon couleur du rôle), avatar et présence, pseudo de serveur et
  d'origine, badges, statut et jeu, exclusion, rôles, dates d'arrivée sur le serveur et sur Estompe. Échap, le fond ou la croix la ferment. Sur ma carte,
  « Modifier mon profil » la ferme et ouvre le compte des paramètres (voir [Profil](#profil)) ; « Envoyer un message » est marqué « bientôt » ; « Modérer » ouvre
  les actions de modération permises (voir [Modération](#modération)) ; modification des rôles et historique ne sont pas dans l'app native.
- Badges de la carte (`BadgeCarte`) : 24 px dans une case de 32, infobulle du nom (gras) et de la description au survol ou au focus (Tab), au-dessus du badge.

#### Avatars et badges

`Profils` (`src/coquille/avatars.rs`) télécharge par le client HTTP (`GET /users/{id}/avatar`, `GET /servers/{id}/users/{uid}/avatar` quand le serveur a une photo
propre au membre, PNG ou JPEG ; `GET /users/{id}/badges`). Un avatar et des badges ne sont demandés que pour les lignes de la fenêtre, une fois que la liste a donné sa hauteur ;
un avatar en échec n'est pas redemandé pendant la session.

| Cache | Clé | Borne |
|---|---|---|
| disque, `<cache>/avatars/` (`CacheDisque`, crate client) | `u.<utilisateur>.v<version>` ou `s.<serveur>.u.<utilisateur>.v<version>` | 64 Mo (`BORNE_DISQUE`), éviction du moins récemment lu |
| mémoire, avatars décodés | utilisateur, version, serveur éventuel | 256 images de 96 × 96 au plus (`CAPACITE_AVATARS`), LRU |
| mémoire, badges | utilisateur | 1 024 listes (`CAPACITE_BADGES`), LRU |
| modèle de la liste | — | seules les lignes de la fenêtre portent leur image ; elle est retirée quand la ligne en sort |

- Le cache disque garde les octets reçus ; l'ordre des lectures est la date de modification des fichiers, relue au redémarrage. Écriture dans un fichier
  temporaire renommé ; un fichier temporaire restant est supprimé à l'ouverture, un fichier qui ne se décode pas est supprimé et redemandé, un nom de fichier invalide est ignoré.
- Une image est recadrée en carré, réduite à 96 px et arrondie dans ses pixels (`arrondir`) : les deux moteurs de rendu l'affichent de la même façon.
- Sans image, les initiales restent affichées.
- Badges (`Medaille`, membres, messages et carte) : image du badge si `icon_version` > 0 (`GET /badges/{id}/icon`, SVG rendu par Slint, WebP, PNG ou JPEG réduits à 64 px),
  sinon son emoji en Twemoji embarqué, sinon une médaille de la couleur du badge. Les icônes sont demandées avec la liste des badges de la personne, une fois par badge et
  par version (`CAPACITE_ICONES_BADGES`, 256 en mémoire, un échec n'est pas redemandé) ; les images décodées sont gardées par le fil de l'interface.

### Recherche et palette

Une seule fenêtre, comme `SearchModal.svelte` : Ctrl+K (depuis la coquille, aucun modal ouvert), le bouton « Recherche globale » du rail ou la loupe de l'en-tête
(« Rechercher ou aller à… »). Interface `ui/coquille/recherche.slint` (global `DonneesRecherche`), état `EtatRecherche` et projections pures dans `src/coquille/recherche.rs`,
actions dans `src/coquille/pont/recherche.rs` ; texte sans interface dans `estompe_client::recherche` (tests repris de `search.test.ts`).

- Saisie : filtres de l'API (`de:`, `dans:#`, `serveur:`, `mentionne:`, `avant:`, `après:`, `pendant:`, `a:`, `épinglé:`, `"expression"`, `-exclu`), proposés pour le dernier mot
  (`suggestions` : clés, puis pseudos, salons texte, serveurs ou valeurs de `a:` selon la portée, sans accents ni casse) ; flèches haut et bas (en boucle) pour parcourir, Tab
  complète le filtre actif (curseur en fin de saisie), Entrée ouvre l'entrée active ou lance la recherche, Échap ferme et rend le focus au champ de saisie (sinon à la coquille).
- Palette « Aller à » (`aller_a`, mêmes règles que le front) : accueil, serveurs (dernier salon vu), sections ouvertes, salons de tous les serveurs, conversations privées ;
  `#` limite aux salons, `@` aux personnes (conversations privées) ; 8 entrées pour un texte, 50 pour un préfixe seul ; rien si la saisie contient un filtre. L'entrée active reste
  visible (défilement calculé en Rust). Actions (`actions` de la crate client, `actions_de_la_palette`) : « Nouveau message privé » et « Statut : … » (le statut actuel porte « actuel »),
  toutes avec `>`, 8 au plus pour un texte ; celles du vocal, du profil et des paramètres arrivent avec leurs lots.
- Recherche (`RechercheMessages`, `Client::chercher_messages`) : `GET /search?q=&sort=&offset=` avec `server_id`, `channel_id` ou `dm_id` ; portées « Partout », le serveur courant et
  la conversation ouverte (choisie par défaut), tri « Pertinence » ou « Plus récents » ; changer l'une ou l'autre relance la recherche. Une réponse d'une recherche remplacée est
  ignorée ; la page suivante est demandée à 200 px du bas (`MARGE_DE_SUITE`) tant que `has_more`.
- Résultats groupés par conversation (« Serveur › # salon » ou « @ pseudo », dans l'ordre de première apparition), auteur, jour et heure, extrait de 180 caractères centré sur le
  premier terme (`extrait`, termes sans filtres ni exclusions, racines : « réunions » surligne « réunion »), termes sur fond rose, nombre de fichiers. Un clic ou Entrée sur un
  résultat ferme la fenêtre, ouvre la conversation puis `Store::sauter_a` : chargement autour du message s'il est hors de la fenêtre chargée, mise en évidence 1,5 s.
- États : « Aucun résultat pour « … ». » (et « Essaie d'élargir à « Partout ». » hors de « Partout »), erreur du serveur affichée dans la fenêtre (« Recherche impossible : … »),
  dernières recherches (10, `recherches_recentes` des préférences, « Effacer »), sinon l'aide des filtres.
- Écarts : le front affiche une erreur dans une notification, l'app native dans la fenêtre ; un extrait s'affiche sur une ligne continue (retours à la ligne du message remplacés
  par des espaces) ; une saisie vide garde l'aide des filtres au lieu des six premières actions du front.

### Nouvelle conversation, accueil et statut

- « Nouveau message privé » (`ui/coquille/nouveau-prive.slint`, global `DonneesNouveauPrive` ; état `EtatNouveauPrive` et actions dans `src/coquille/pont/nouveau_prive.rs`),
  ouvert par le « + » des conversations du panneau ou l'action de la palette : champ « Pseudo » qui propose 6 pseudos connus au plus (`estompe_client::nouveau_prive` :
  conversations existantes puis membres des serveurs, sans soi, sans accents ni casse, le pseudo exact exclu) avec la présence et l'origine ; flèches, Entrée ou clic
  choisissent, Échap ferme la liste puis la fenêtre. « Ouvrir la conversation » appelle `Store::ouvrir_message_prive` (`POST /dms`) : le serveur rend la conversation existante
  s'il y en a une, ajoutée en tête de la liste seulement si elle n'y est pas, puis la vue l'ouvre ; `user_not_found`, `self_dm` et les autres refus s'affichent dans la fenêtre.
  Pas de conversation de groupe : l'API ne prend qu'un pseudo, comme le front.
- Accueil : « Tout marquer lu » (`Store::tout_lire`, `tout_lire` du réducteur) lit toutes les conversations à rattraper (mentions comptées et non-lus, sauf la conversation
  ouverte) : pastilles et mentions retirées tout de suite, `POST …/read` par conversation et un seul `POST /notifications/read` ; les autres appareils reçoivent `read.updated`.
  Carte « En ligne » (`lignes_contacts`) : les conversations privées dont la personne n'est pas hors ligne, avec « En vocal · salon », son jeu ou son statut, et « N sur M » ;
  un clic ouvre la conversation.
- Barre de l'utilisateur : pastille de présence et statut (ou jeu en cours) sous le pseudo (`donnees_compte`) ; un clic, Entrée, la touche Menu ou Maj+F10 ouvrent le menu
  (`menu_moi`) : « En ligne », « Absent », « Ne pas déranger » (statut actuel coché, `PUT /me/status` par `Store::definir_statut`, optimiste et rétabli en cas d'erreur, que la zone
  de saisie signale), puis « Voir mon profil », « Modifier mon profil » et « Signaler un bug » marqués « bientôt ». Le changement arrive aux autres par `status.updated`.
  Le statut « invisible » n'existe ni dans l'API ni dans le front.

### Créer ou rejoindre un serveur, créer un salon

Fenêtres de `ui/coquille/creation.slint` (globaux `DonneesCreerRejoindre` et `DonneesCreationSalon`), actions dans `src/coquille/pont/creation.rs`, règles sans
interface dans `estompe_client::creation` (équivalent de `channel-create.svelte.ts`), appels par le store (`Store::creer_serveur`, `rejoindre_serveur`, `quitter_serveur`,
`creer_salon`), comme `CreateJoinModal.svelte` et `CreateChannelModal.svelte` :

- « Créer ou rejoindre un serveur » : ouverte par le « + » du rail, « Pour commencer » de l'accueil ou l'action de la palette ; la partie création n'apparaît qu'avec
  `servers.create`, la partie invitation qu'avec `servers.join` (capacités de `GET /me`). Nom de 50 caractères au plus avec l'aperçu de ses initiales ; « Choisir une
  icône » ouvre le sélecteur de fichiers puis le recadrage du profil (« Recadrer l'icône du serveur », même carré de 256 px) : l'icône part par `PUT /servers/{id}/icon`
  juste après `POST /servers` (le front ne propose l'icône que dans les réglages du serveur) ; si elle échoue, le serveur est créé et ouvert et la fenêtre le signale.
  Lien complet ou code seul (`code_d_invitation`), `POST /server-invites/{code}/join`, « Invitation invalide ou révoquée. » sur un 404. Le serveur créé ou rejoint est
  rechargé (`GET /servers`, puis son détail) et ouvert.
- « Quitter le serveur » : dans la vue d'ensemble des réglages du serveur, comme le front, avec confirmation, absent pour son créateur (l'API refuse `owner_cannot_leave`) ;
  `POST /servers/{id}/leave`, puis le serveur est oublié sans attendre `member.left` et la page se referme.
- « Créer un salon » : « + » de l'en-tête du panneau et de chaque groupe (texte, esquisses, vocaux), « Créer un salon » du menu d'un salon (même type, placé après lui
  par `after`) ou d'un serveur. Rien de tout cela sans le droit `manage_channels` du serveur (`peut_creer_salon`), suivi en temps réel (`Changement::Roles` reprojette
  l'en-tête). Types texte, vocal, vidéos et musique, cinéma seulement si Entracte est branché (`cinema` de `GET /me`) ; nom obligatoire, 30 caractères, sans caractère
  de contrôle, unique pour son type sans casse (`erreur_nom_salon`), message affiché dès la première saisie, erreur du serveur à la place ; salon privé avec rôles
  (sans le rôle par défaut) et membres (sans soi) ayant accès ; réglages avancés : sujet (200 caractères), mode lent pour un salon texte, limite de participants pour un
  vocal ou un cinéma. La requête ne porte que les réglages du type (`NouveauSalon::requete`). Le salon créé est ajouté au store (comme `channel.created`) et ouvert ;
  ceux des autres arrivent par `channel.created`.
- Pas de catégories : ni le front ni l'API n'en ont, les salons sont groupés par type. La mise en avant passagère du salon créé (`channelCreate.fresh`) et la notification
  « Tu as rejoint » du front ne sont pas reprises (pas de toast).

### Réglages d'un serveur

Page de la zone de contenu, comme `ServerSettingsModal.svelte` : `ui/coquille/reglages-serveur.slint` (global `DonneesReglagesServeur`), actions dans
`src/coquille/pont/reglages.rs`, règles sans interface dans `estompe_client::reglages_serveur` (onglets selon les droits, limites des emoji, nom proposé, recherche des
membres, libellés des invitations et des bans), appels par `Client` (`renommer_serveur`, `supprimer_icone_de_serveur`, `invitations_serveur`, `creer_invitation_serveur`,
`bannis`, `lever_ban`, `ajouter_emoji`, `renommer_emoji`, `supprimer_emoji`) et le store (`Store::renommer_serveur`, `supprimer_serveur`, appliqués sans attendre l'évènement).

- Ouverture : bouton « Paramètres du serveur » de l'en-tête du panneau d'un serveur ou entrée du menu d'un serveur du rail. Comme les paramètres, toute navigation
  la referme, la marge est masquée tant qu'elle est ouverte ; Échap ou « Fermer » rendent le focus à la coquille. Flèches, Origine et Fin passent d'un onglet à l'autre.
- Onglets selon les droits du serveur (`droits`, suivis en temps réel) : « Vue d'ensemble », « Emojis » et « Membres » pour tous, « Invitations » avec `create_invites`,
  « Bannis » avec `ban_members`, « Salons » avec `manage_channels` et « Rôles » avec `manage_roles` (voir [Rôles et permissions](#rôles-et-permissions)), « Journal »
  (`view_audit_log`, voir [Modération](#modération)). L'en-tête dit « Tu es propriétaire », « Tu gères ce serveur » ou « Tu es membre ».
- Vue d'ensemble : nom (50 caractères, `manage_server`) avec compteur, icône (`manage_server` ou administrateur d'Estompe) choisie puis recadrée par le recadrage du profil
  (`PUT /servers/{id}/icon`) ou supprimée (`DELETE /servers/{id}/icon`), aperçu dans la liste des serveurs ; rien ne part avant « Enregistrer » de la barre
  « Modifications non enregistrées » (« Annuler » revient au serveur). Le créateur a « Supprimer le serveur… » : confirmation qui n'est validable qu'avec le nom exact
  du serveur saisi, puis `DELETE /servers/{id}` ; les autres ont « Quitter le serveur ».
- Invitations : expiration (sans, 30 min, 1 h, 24 h, 7 jours) et nombre d'utilisations (illimité, 1, 5, 10, 100) comme le front, « Créer un lien », liens actifs
  (`GET /servers/{id}/invites`) avec auteur, utilisations, expiration, copie du lien complet (`<adresse>/#/join/<code>`) et « Révoquer ».
- Emojis (`GET /emojis`, rechargés à chaque changement et sur `emoji.updated`) : « Ajouter un emoji » ouvre le sélecteur de fichiers (PNG, JPEG, GIF) ; le type (signature
  du fichier), le poids (128 Ko) et les dimensions (256 px, lues dans l'en-tête comme l'API) sont vérifiés avant tout envoi (`verifier_emoji`), avec le message du front.
  Le nom est proposé depuis le fichier (`nom_propose`) et doit suivre `^[a-z0-9_]{2,32}$` ; 50 emoji par serveur au plus (l'ajout disparaît à la limite). Renommer (fenêtre
  de saisie, plutôt que le champ en place du front) et supprimer sont offerts à l'auteur et au créateur du serveur, comme l'API.
- Membres : liste du serveur, créateur en tête puis par nom, rôle le plus haut en couleur, date d'arrivée, recherche sans accents ni casse sur le nom affiché et le
  pseudo ; 200 lignes au plus (le compteur invite à affiner). Le front n'a pas cet onglet ; un membre se modère depuis son menu ou sa carte (voir [Modération](#modération)).
- Bannis : `GET /servers/{id}/bans`, auteur, date, durée (« définitif » ou « jusqu'au … »), raison, « Lever le ban » (`DELETE /servers/{id}/bans/{uid}`).
- Écarts : pas de description du serveur (ni l'API ni le front n'en ont) ; le transfert de propriété et l'ordre des salons restent à faire. Les messages de réussite et d'erreur s'affichent en tête de la page (pas de toast dans l'app native).

### Rôles et permissions

Onglets « Rôles » et « Salons » des réglages d'un serveur, comme `RolesSettings.svelte` et `ChannelPerms.svelte` : `ui/coquille/reglages-roles.slint` (global
`DonneesRoles`), actions dans `src/coquille/pont/roles.rs` (état `EtatRoles` dans celui de la page), règles sans interface dans `estompe_client::gestion_roles`
(`Contexte` : rang, rôle modifiable ou déplaçable, permission accordable, membre gérable, nouvel ordre, permissions perdues ; `BrouillonRole` ; trois états d'une
surcharge), appels par `Client` (`creer_role`, `modifier_role`, `supprimer_role`, `ordonner_roles`, `attribuer_role`, `surcharges_du_salon`, `definir_surcharge`).
Après chaque modification, le détail du serveur est rechargé (`Store::recharger_serveur`) sans attendre `roles.updated`, qui fait de même chez les autres membres.

- Liste : rôles du plus haut au plus bas puis « @tout le monde », pastille de couleur, icône, « séparé » ; « Nouveau rôle » (50 rôles au plus) crée un rôle placé juste
  au-dessus de « @tout le monde » et l'ouvre. Ordre par glisser-déposer d'une ligne ou par les flèches « Monter » et « Descendre » ; seuls les rôles sous le sien bougent
  et aucun ne passe au-dessus d'un rôle figé (`PUT /servers/{id}/roles-order`).
- Éditeur : nom (30 caractères), icône (8 caractères), couleur (palette, saisie `#rrggbb` ou « Aucune »), « Afficher séparément dans la liste des membres »,
  membres du rôle (retrait, ajout par une recherche sans accents qui propose six membres gérables), puis les 25 permissions groupées comme le front avec un filtre sans
  accents ; seules celles qu'on a soi-même sont modifiables. « @tout le monde » n'a que ses permissions. La barre du bas porte « Supprimer le rôle » (confirmé) et
  « Enregistrer », actif dès qu'une valeur diffère et que le brouillon est valide (message sinon).
- Garde-fous : un rôle au niveau du sien ou au-dessus est en lecture seule (le créateur gère tout) ; enregistrer un rôle qu'on a (ou « @tout le monde »), le supprimer
  ou se le retirer, ou une surcharge de salon, qui feraient perdre une de ses propres permissions demandent une confirmation qui les nomme (le front ne le fait pas).
- Salons : liste groupée par type comme le front (texte, esquisses, vocaux, cinéma), « verrouillé », limite de participants, « Créer un salon » ; « Permissions »
  déplie sous le salon le choix du rôle (menu) et les permissions de son type en trois états (hériter, autoriser, refuser), modifiables seulement pour un rôle sous le
  sien et une permission qu'on a (`PUT /servers/{id}/channels/{cid}/overrides/{rid}`), rechargées à chaque `roles.updated`.
- Écarts : pas de surcharge par membre (ni route dans l'API, ni écran dans le front : `channel_member_overrides` n'est rempli qu'à la création d'un salon privé) ;
  la liste est au-dessus de l'éditeur (la zone de contenu est trop étroite pour deux colonnes, comme le front sous 760 px), les membres passent avant les
  permissions et les actions restent dans une barre fixe ; « Gérer les rôles » du menu d'un membre (carte de profil du front) n'est pas repris.

### Modération

Comme `ModerationModal.svelte`, `ChannelModModal.svelte`, `lib/moderation.svelte.ts` et le journal de `ServerSettingsModal.svelte` : fenêtres
`ui/coquille/moderation.slint` (globals `DonneesModeration` et `DonneesModerationSalon`), actions dans `src/coquille/pont/moderation.rs`, règles sans interface
dans `estompe_client::moderation` (actions permises, durées, purges, validation, libellés, phrases, icônes et tons du journal, texte d'exclusion), appels par `Client`
(`expulser_membre`, `bannir_membre`, `exclure_membre`, `lever_exclusion`, `avertir_membre`, `renommer_membre`, `journal_du_serveur`, `moderer_salon`, `renommer_salon`,
`supprimer_salon`, `supprimer_derniers_messages`).

- Membre : le menu d'un membre (liste des membres) et le bouton « Modérer » de sa carte de profil proposent, dans l'ordre du front, « Exclure temporairement… » ou
  « Lever l'exclusion temporaire » (`timeout_members`), « Avertir… » (`timeout_members`), « Changer le pseudo… » (`manage_nicknames`), « Expulser… »
  (`kick_members`) et « Bannir… » (`ban_members`), ces deux derniers en rouge ; seulement sur un membre de rang strictement inférieur au sien (`roles::domine` :
  le créateur domine tout le monde, personne ne le modère, ni soi-même). « Lever l'exclusion » part aussitôt (`DELETE /servers/{id}/members/{uid}/timeout`), une
  erreur s'affiche sous la zone de saisie.
- Fenêtre de modération d'un membre : avatar et explication du front, durée en puces (exclusion : 60 s à 1 semaine, bannissement : définitif à 7 jours) et
  « Personnalisé… » en heures (28 jours au plus pour une exclusion, 10 ans pour un ban), purge des messages récents pour un ban, raison de 200 caractères (obligatoire
  pour avertir), pseudo de serveur de 32 caractères pour le renommage (vide : pseudo d'origine). Le bouton est rouge et nommé (« Bannir dave ») pour expulser et
  bannir, « Confirmer » sinon ; la réussite (« Membre banni »…) ou l'erreur s'affiche dans la fenêtre, qui ne garde alors que « Fermer ».
- Salon : « Modération du salon… » du menu d'un salon texte, avec `manage_channels` ou `manage_messages` sur ce salon. Avec `manage_channels` : nom (30 caractères,
  mêmes règles qu'à la création, son propre nom ignoré), mode lent, verrouillage, « Enregistrer » (`PATCH` du nom s'il change puis
  `PUT /servers/{id}/channels/{cid}/moderation`, détail du serveur rechargé) et « Supprimer le salon… » (confirmation). Avec `manage_messages` : suppression des N
  derniers messages (1 à 100) de tout le monde ou d'un auteur choisi par un menu, confirmée, puis le nombre supprimé est affiché (`POST /channels/{cid}/messages/bulk-delete`).
- Onglet « Salons » des réglages : chaque salon a aussi « Renommer » (fenêtre de saisie) et « Supprimer » (confirmation), comme le menu du front.
- Onglet « Journal » des réglages (`view_audit_log`) : `GET /servers/{id}/audit-log`, rechargé à l'ouverture de l'onglet et à chaque changement du filtre (menu :
  toutes les actions, modération des membres, rôles, salons, vocal, messages, serveur) ; chaque ligne : date, icône et ton du front (rouge pour ban, expulsion et
  messages, orange pour exclusion et avertissement), auteur et phrase (« a banni « hugo » — spam »), « Aucune action enregistrée. » sinon.
- Membre exclu : la carte de profil affiche « Exclu temporairement jusqu'au … » ; pour soi, la zone de saisie passe en lecture seule avec
  « Tu es exclu temporairement de ce serveur jusqu'au … » au lieu du texte d'absence de permission (le serveur retire `send_messages` ; `member.timeout` me visant
  recharge le détail du serveur).
- Écarts : le sujet d'un salon ne se modifie pas (ni l'API ni le front ne le permettent après la création) ; la raison est un champ d'une ligne (zone de texte dans le
  front) ; la réussite s'affiche dans la fenêtre au lieu d'un toast.

## Messagerie

Lecture des salons texte et des conversations privées (`src/messagerie/`, `ui/messagerie/`). La logique sans interface est celle de la crate client :
store des messages (`store/messages/`, pagination, fenêtre de 600 messages par conversation, 8 conversations en cache), markdown (`markdown/`),
mentions, regroupement et séparateurs (`format::lignes_de_messages`).

| Fichier | Rôle |
|---|---|
| `src/messagerie/lignes.rs` | projection pure : conversation du store → lignes (séparateurs de jour, « Nouveaux messages », message avec auteur, couleur du rôle, avatar, heure, réponse citée, modifié, épinglé, système, bot, mention, mise en évidence, annexes) ; premier non-lu, mentionnables d'une conversation |
| `src/messagerie/rendu.rs` | markdown aplati pour Slint : chaque `Bloc` devient un `BlocAffiche` (paragraphe, titre, code ; profondeur de citation, retrait et puce de liste), les segments d'un paragraphe un balisage `StyledText` ; cibles des liens (`u:`, `m:`, `s:`) |
| `src/messagerie/defilement.rs` | virtualisation : hauteurs mesurées ou estimées, ancre, collage en bas, saut à une ligne |
| `src/messagerie/pont.rs` (`PontMessagerie`) | applique les `Changement::Messages` aux modèles, cache borné des rendus (512, LRU), chargement des pages, avatars et badges des auteurs, images, aperçus et emoji de serveur des lignes de la fenêtre, animations, visionneuse, téléchargements, marquage comme lu, saisie en cours |
| `src/messagerie/emoji.rs` | paquet d'emoji embarqué, découpe du texte en texte et emoji (séquences composées comprises), images des emoji |
| `src/messagerie/medias.rs` (`Medias`) | téléchargement, décodage hors du fil d'interface et caches des images (pièces jointes, emoji de serveur, images d'aperçu), liste des emoji de serveur, aperçus de liens |
| `src/messagerie/actions.rs`, `src/messagerie/pont/actions.rs` | droits d'un message et entrées de son menu (purs) ; exécution des actions (réponse, modification, épingle, réaction, copie, non-lu, suppression confirmée) |
| `src/messagerie/pont/sondages.rs`, `ui/messagerie/creation-sondage.slint` | création d'un sondage (formulaire, validation, publication), vote, clôture, suppression et votants |
| `src/messagerie/annexes.rs` | projections pures : carte d'aperçu d'un lien, taille d'une vignette avant chargement, chemin sûr d'un fichier enregistré |
| `src/messagerie/vocal.rs`, `src/messagerie/pont/vocal.rs`, `ui/messagerie/vocal.slint` | messages vocaux : enregistrement (micro, encodage hors du fil d'interface), écoute avant l'envoi, envoi, lecture d'un message reçu (voir [Messages vocaux](#messages-vocaux)) |
| `ui/messagerie/vue.slint`, `element.slint`, `annexes.slint`, `visionneuse.slint`, `epingles.slint`, `donnees.slint` | liste, élément de message, pièces jointes, cartes, sondage, réactions et « Vu par », visionneuse, messages épinglés, global `DonneesMessagerie` |

### Virtualisation

`ListView` de Slint 1.18 instancie les lignes visibles et accepte des hauteurs différentes, mais estime la position par la hauteur moyenne des
lignes instanciées (`update_visible_instances`, `i-slint-core/model/repeater.rs`) : la hauteur du contenu et la barre de défilement changent à
chaque pas, et des lignes ajoutées au-dessus de la première instance (cas d'une page plus ancienne quand on est près du haut) décalent la vue.
L'app virtualise donc elle-même, avec `virtualisation::fenetre_de_hauteurs` de la crate client :

- le modèle `DonneesMessagerie.lignes` ne contient que les lignes de la fenêtre (l'écran et 600 px au-dessus et au-dessous) ; deux espaces
  (`haut`, `bas`) remplacent les autres dans un `Flickable` ;
- chaque ligne signale sa hauteur (`init` et `changed height` → `ligne-mesuree`) ; avant la mesure, une estimation tirée du texte, des annexes et de la largeur ;
  un changement de largeur oublie les mesures et incrémente `DonneesMessagerie.mesure`, qui fait signaler à nouveau leur hauteur aux lignes instanciées ;
- une ancre (ligne sous le haut de l'écran et décalage) est rétablie après chaque changement : ajout en tête, mesure d'une ligne au-dessus,
  retrait par la borne du store. Collée en bas, la liste suit les nouveaux messages ; en lecture de l'historique, un message reçu ne la déplace pas
  et le bouton « N nouveaux messages » apparaît. La liste n'est collée que tout en bas (à moins de 1 px) : un cran de molette depuis le bas
  n'est plus ramené en bas à la mesure suivante ; à moins de 80 px du bas, un message ajouté en fin la recolle ;
- défilement à la molette, au pavé tactile ou par la barre seulement : `mouse-drag-pan-enabled: false` sur les `Flickable` (le glisser à la
  souris, actif par défaut dans Slint, faisait bouger la liste avec de l'inertie). `BarreDefilement` (`ui/composants/defilement.slint`, commune à
  `Defilement`, `ListeVirtuelle` et la liste des messages) apparaît au survol et pendant un geste (`flicked`, masquée 600 ms après le dernier), et se tire ;
- les pages plus anciennes ou plus récentes sont demandées à moins de 1 500 px d'un bord ; une ligne « Chargement des messages… » ou
  « Début de la conversation » occupe le bord ;
- à l'ouverture, la vue se place sur le premier non-lu (nombre de non-lus lu avant que le store ne les efface) ; sans non-lu, en bas ;
- les rappels de mesure et de défilement planifient un rafraîchissement au tour suivant (`slint::Timer` de durée nulle) : un rappel `init` qui
  modifierait le modèle pendant l'instanciation de ses lignes ferait paniquer le répéteur.

Mises à jour fines : `synchroniser` compare la fenêtre à son ombre ; une modification, un épinglage ou une réaction ne remplacent que la ligne du message,
une suppression la retire. La conversation entière est reprojetée côté Rust (au plus 600 messages), jamais le modèle Slint.

### Rendu du markdown, sélection et copie

Un paragraphe est un `StyledText` (Slint 1.18) : gras, italique, barré, liens. Un paragraphe qui contient un emoji, un emoji de serveur, du code
en ligne ou un passage masqué devient une suite de segments dans un `FlexboxLayout` qui passe à la ligne (`TexteEnSegments`, `ui/composants/texte-riche.slint`) :
un `StyledText` par mot (styles et liens gardés), une image par emoji, une capsule en JetBrains Mono par code en ligne (`StyledText` dessinerait le
code dans la police à chasse fixe du système), un pavé d'encre cliquable par mot masqué (comme `.spoiler` du front). Le balisage est produit depuis le `Rendu` de la crate
client, texte échappé et délimiteurs entourés d'une espace sans chasse pour que les bords d'un style (espace, ponctuation) ne le cassent pas.
Titres, blocs de code, citations (barres) et listes (puces, retraits) sont des éléments Slint autour. Les mentions de personne sont des liens
(`m:` → carte de profil), celles de rôle ou de groupe sont colorées ; un passage masqué est un pavé d'encre cliquable (`s:` → révélé pour ce message) ;
un lien (`u:`) n'est ouvert par `OuvertureUrl` que si `lien_ouvrable` l'accepte, au clic comme à l'analyse.

Sélection : ni `Text` ni `StyledText` ne permettent de sélectionner du texte en 1.18 (seul `TextInput`, d'un seul style, le fait). La copie se fait
donc par message entier : bouton « Copier le message » au survol (toujours instancié, affiché si la ligne est survolée, a le focus ou si le bouton
lui-même est survolé : il est dessiné au-dessus de la zone de la ligne, qui perd alors le survol), ou Ctrl+C sur le message qui a le focus ; le texte copié est `texte_brut`, passé
au presse-papiers par un `TextInput` caché (`select-all()`, `copy()`).

L'italique vient de la face italique d'Instrument Sans (`polices/InstrumentSans-Italic.ttf`, même source et même licence que la face droite). Écarts : un lien est toujours souligné ; dans un paragraphe
en segments, un mot plus long que la ligne est coupé dans son segment au lieu de passer à la ligne entier, et un bloc de code en ligne ne se coupe pas.

### Élément de message et emplacements à venir

`CorpsMessage` (`ui/messagerie/element.slint`) : gouttière (avatar 36 px, ou heure au survol pour une suite groupée), réponse citée (clic :
`sauter_a`, chargement autour du message si besoin, puis mise en évidence 1,5 s), en-tête (pseudo à la couleur du rôle, « Bot », deux badges,
épingle, heure), corps (`TexteRiche`, 15 px), annexes, état d'envoi.

- Annexes : `annexes()` (`lignes.rs`, `DonneesAnnexes` : pièces, sondage, réactions, premier lien) est pure ; `PontMessagerie` y ajoute l'état
  des médias de la ligne (image chargée, aperçu reçu, téléchargement) et `annexes_slint()` en fait un `AnnexesMessage`, affiché par
  `AnnexesMessageVue` (`ui/messagerie/annexes.slint`) ; `resume` sert au lecteur d'écran. Un message avec sondage affiche la carte du sondage à la place
  du corps, des pièces jointes et de l'aperçu, comme `MessageItem.svelte`. Une image chargée, un aperçu reçu ou un téléchargement qui avance ne remplacent que la ligne du message.
- État d'envoi : champs `etat-envoi` (« Envoi… », « En attente du réseau… », « Non envoyé · raison ») et `envoi-echec` de `LigneMessage`, vides pour un message du serveur ; « Réessayer » et « Supprimer » sous un message refusé (voir [Écriture](#écriture)).
- Accusés de lecture : `Store::lecteurs` rattache chaque lecture au dernier message lu de la fenêtre ; `ContexteLignes::lecteurs` les ajoute aux annexes
  (`DonneesLecteur` : pseudo, initiales, avatar, heure de lecture). `LecteursVue` (`annexes.slint`) montre cinq avatars et « +N » à droite des réactions ;
  un clic déplie « Vu par » avec chaque personne et son heure, « Fermer » la replie. Un `read.updated` reprojette la conversation ouverte.

### Emoji

Le moteur logiciel de Slint (captures) ne dessine les glyphes qu'en niveaux de gris (`swash`, contours seuls) ; le moteur Skia (l'app) dessinerait une
police couleur, mais chacun la sienne, et la police d'emoji du système change d'un OS à l'autre. Les emoji sont donc des **images**, identiques dans les
deux moteurs et sur les deux OS :

- `emoji/twemoji.bin` : les 3 786 images SVG de [Twemoji](https://github.com/jdecked/twemoji) v17.0.3 (dessins sous licence CC-BY 4.0, attribution dans
  « À propos ») pour les 4 739 séquences d'emoji-data v16.0.0 (MIT) : teintes de peau, drapeaux, séquences ZWJ, touches. SVG regroupés par blocs de 32
  compressés (deflate) : 9,1 Mo de SVG, **1,9 Mo** dans le binaire ; un bloc est décompressé à la demande (4 blocs gardés), une image rendue par
  Slint (resvg) à sa taille d'affichage et gardée (512 images, LRU, fil d'interface).
- Régénérer : `just estompe-natif-emoji` (`scripts/estompe/natif-emoji.mjs`, télécharge `emoji.json` d'emoji-data et les SVG depuis `raw.githubusercontent.com`).
- Découpe (`emoji::decouper`) : plus longue séquence connue du paquet à chaque position ; un caractère à présentation texte par défaut (©, ®, ❤ sans
  sélecteur `FE0F`) reste du texte, comme dans un navigateur.
- Emoji de serveur : `GET /emojis` à l'ouverture de la première conversation et à chaque `emoji.updated` (`Changement::Emojis`) ; un `:nom:` connu
  devient une image (`GET /emojis/{id}/image`, celle du serveur courant d'abord, animée si c'est un GIF ou un WebP animé), un nom inconnu reste du texte.
- Message composé seulement d'emoji (27 au plus, espaces et retours à la ligne compris) : emoji de 48 px au lieu de 1,375 fois le corps (20 px). Le
  front n'a pas de règle équivalente (les emoji y gardent la taille du texte) : c'est un ajout de l'app native, demandé par #302.
- Réactions : pilule de 28 px, emoji en image, compte en JetBrains Mono, « la mienne » en rose avec un trait rose foncé ; au survol, la liste des
  personnes (`titre_de_reaction`, sans l'emoji final, déjà dessiné) ; le lecteur d'écran lit le titre complet. Mise à jour en direct par `reaction.updated`.

### Pièces jointes

| Genre (`categorie`) | Affichage |
|---|---|
| image (`image/png`, `image/jpeg`, `image/gif`, `image/webp`, comme `AttachmentView.svelte` ; AVIF non affiché par le front, donc pas pris en charge) | vignette dans 360 × 300, coins arrondis dans ses pixels ; clic : visionneuse |
| fichier | puce : nom, taille ; clic : téléchargement avec progression, puis « Ouvrir » |
| message vocal | lecteur (voir [Messages vocaux](#messages-vocaux)) ; « illisible sur cet appareil » avec « Télécharger » si le fichier ne se décode pas |
| vidéo, replay | puce « bientôt dans l'app native » (lot 9) |
| expiré | puce barrée « (fichier expiré) » |

- Avant le chargement, la vignette a sa taille définitive si les dimensions d'origine sont connues (mémorisées au premier décodage, 4 096 sources,
  LRU) ; sinon 240 × 180. L'API n'annonce pas les dimensions d'une pièce jointe : la première ouverture d'une image peut donc décaler la ligne
  (l'ancre de défilement la compense).
- Téléchargement et décodage seulement pour les lignes de la fenêtre ; décodage par `spawn_blocking` (hors du fil d'interface et du runtime), deux à la
  fois au plus (`DECODAGES_SIMULTANES`), avec des limites (`COTE_DECODABLE_MAX` 16 384 px, `ALLOCATION_DECODAGE_MAX` 512 Mo) ; l'image est réduite à
  sa taille d'affichage aussitôt décodée, seule la réduite est gardée.
- Animations (GIF, WebP animé ; emoji de serveur animés) : toutes les images décodées au chargement, réduites (240 au plus, la moitié du plafond
  mémoire au plus), délai minimal de 20 ms (en dessous : 100 ms, comme les navigateurs). Une seule minuterie pour toute la liste, armée sur la
  prochaine image due, et seulement tant qu'une image animée est **à l'écran** (`Defilement::visible`, pas seulement dans la fenêtre de 600 px) :
  hors de l'écran, ni minuterie ni réveil. Une image change par `set_row_data` du modèle de pièces (ou de segments) de la ligne, sans la remplacer.
- Visionneuse (`ui/messagerie/visionneuse.slint`) : image décodée dans 2048 × 2048 (fixe pour un GIF), zoom de 100 % à 800 % (boutons, + et -, 0,
  double clic), déplacement quand elle dépasse, image précédente et suivante du message (flèches), copie (Ctrl+C, image d'origine décodée à la
  demande, `PressePapiers` de la crate plateforme), enregistrement (comme un fichier), Échap ou le fond pour fermer.
- Enregistrement : `Fichiers::choisir_dossier` (crate plateforme, `rfd` : sélecteur de Windows, portail XDG sous Linux) ; sans sélecteur ou en
  erreur, le dossier Téléchargements (`dossier_telechargements` : `%USERPROFILE%\Downloads`, `XDG_DOWNLOAD_DIR` ou `~/Downloads` ; sous
  `ESTOMPE_NATIF_DOSSIER`, `<dossier>/telechargements`). Nom = `fichiers::nom_de_fichier_sur`, suffixe « (n) » si le fichier existe (`chemin_libre`) ;
  écriture dans `<nom>.partiel` renommé à la fin (`Client::telecharger`, `GET /files/{id}?download=1` en flux) ; « Ouvrir » passe par `Fichiers::ouvrir`.

| Cache | Clé | Borne |
|---|---|---|
| disque, `<cache>/medias/` (`CacheDisque`) | `f.<pièce>`, `e.<emoji>`, `x.<empreinte de l'URL>` | 256 Mo (`BORNE_DISQUE_MEDIAS`), moins récemment lu évincé |
| mémoire, images décodées (vignettes, visionneuse, emoji de serveur, images d'aperçu, toutes les images d'une animation) | source et format | 96 Mo (`PLAFOND_MEMOIRE_IMAGES`), en octets, LRU ; une image plus grosse que le plafond n'est pas gardée |
| mémoire, aperçus de liens | message et lien (`cle_de_cache`) | 200 (`TAILLE_DU_CACHE`), LRU, réponse vide comprise |
| images des emoji Unicode | séquence | 512 images rendues, 4 blocs décompressés |

### Aperçus de liens et sondages

- Le premier lien du message (si la préférence « aperçus » est active) est demandé à `GET /link-preview?url=&message=` (`Client::apercu_de_lien`,
  404 = pas d'aperçu) ; la carte suit `choisir_carte` : lien (site, titre, description sur deux lignes, image à droite recadrée en carré), YouTube
  (miniature `img.youtube.com` et titre de l'aperçu ; le clic ouvre le lien dans le navigateur, le lecteur arrive au lot 9), Sillon (genre, titre,
  artiste ; ni pochette ni « Écouter » avant la section Sillon du lot 10). Les images d'aperçu viennent de leur site (`Client::octets_externes`,
  sans jeton, 8 Mo au plus).
- Sondage : question (markdown), « Sondage lancé par », choix avec barre, pourcentage et votes, mon choix (coche), gagnants (trophée) une fois clos,
  pied (`libelle_du_pied`). `poll.voted` et `poll.closed` remplacent la seule ligne du sondage ; l'échéance réveille la liste (une minuterie sur la
  prochaine clôture visible). Voter, clore, supprimer et « Voir les votants » : voir [Sondages](#sondages).

### Temps réel et lecture

- Nouveau message, modification, suppression, épinglage, réaction, vote, saisie (« carol est en train d'écrire… », expirée au bout de 5 s par la boucle du store, testé en temps simulé) : appliqués par `PontMessagerie::appliquer`.
- Lu : `signaler_lecture(en_bas, ancre)` quand le bas de la liste est atteint ou quitté, `definir_focus` quand la fenêtre gagne ou perd le focus.
  Le focus vient de l'évènement `Focused` de winit (`on_winit_window_event`, feature `unstable-winit-030` de Slint), sans code propre à un OS ;
  `Coquille::fenetre_active` est le même chemin, utilisé par les tests. Une lecture sur un autre appareil (`read.updated` à mon nom) retire la pastille.
- Après une coupure (`Signal::Reconnecte`), `rattraper_messages` relit chaque conversation chargée : la fenêtre entière (`[premier, dernier]`,
  pages de 100 depuis `premier − 1`) est comparée au serveur (`Installation::Relecture`) : un message changé (modification, réaction, vote, épingle)
  est remplacé, un message absent est retiré, un message manquant inséré. Puis, si la fenêtre touche le présent, les messages arrivés depuis
  `dernier` sont ajoutés comme avant (au-delà de 500, la conversation est rechargée). Les accusés de lecture et la liste des épinglés ouverte sont rechargés aussi.
- Le compte « Estompe » (notes de version) ouvre une conversation en lecture seule.

### Épinglés

- La liste vit dans le store (`EtatMessages::epingles`, une seule conversation à la fois) : `charger_epingles` (`GET /channels/{id}/pins` ou
  `/dms/{id}/pins`, les plus récemment épinglés d'abord), puis chaque `message.updated` l'ajoute en tête, la remplace ou la retire selon
  `pinned_at`, et `message.deleted` la retire, même pour un message hors de la fenêtre chargée. `ChangementMessages::Epingles` prévient l'interface.
- Onglet de la marge : bouton épingle de l'en-tête (`basculer_epingles` : ouvre l'onglet, un second clic revient à l'onglet et à l'ouverture d'avant,
  comme `basculerEpingles` du web) ou onglet « Épinglés ». Dans un message privé, la marge ne s'ouvre que sur cet onglet (`onglet_effectif`).
- `PontMessagerie::montrer_epingles` charge la liste quand l'onglet devient visible ou que la conversation change, et l'oublie quand il se ferme ;
  chaque carte (`CarteEpingle`, `ui/messagerie/epingles.slint`) donne l'auteur à la couleur de son rôle, la date, le markdown et le nombre de fichiers ;
  « Voir dans la conversation » passe par `sauter_a` (chargement autour du message s'il n'est pas dans la fenêtre, puis mise en évidence).
- « Désépingler » sur chaque carte si le rôle permet d'épingler ce message (`PUT`/`DELETE /messages/{id}/pin`) ; la liste suit par `message.updated`.

### Écriture

- Envoi optimiste (`store/messages/envois.rs`, crate client) : `Store::envoyer` dépose le brouillon dans `EtatMessages::envois` (file unique, gardée même si la conversation sort du cache),
  puis l'expédie dans l'ordre de saisie, un message à la fois (`expedier`, un seul en cours grâce à un verrou). Un envoi local s'affiche aussitôt en fin de conversation
  sous l'identifiant `local:<n>` ; la réponse HTTP le remplace par le message du serveur. Si l'évènement `message.created` arrive avant la réponse, il remplace l'envoi en cours de
  même texte et même réponse ; la réponse HTTP arrivée ensuite ne crée pas de doublon (même identifiant). `ChangementMessages::Envois` prévient l'interface.
- Échec réseau (`Reseau`, délai dépassé) : l'envoi reste « en attente » et la file s'arrête ; elle repart à la reconnexion du temps réel (`Signal::Reconnecte`). Refus du serveur :
  « non envoyé » avec la raison (`RaisonEchec` : droit manquant 403, trop long 413 ou `invalid_message`, limite de débit 429, conversation disparue 404, réponse introuvable,
  pièce jointe invalide), « Réessayer » (`Store::reessayer`) ou « Supprimer » (`Store::abandonner`) ; les envois suivants partent quand même.
- Modification (`PATCH /messages/{id}`, `Store::modifier_message`, réponse appliquée sans attendre l'évènement, réactions gardées), suppression (`DELETE`, retirée à la confirmation,
  gardée en cas d'erreur), « est en train d'écrire » (`POST …/typing`, au plus une fois toutes les 3 s par conversation et seulement pour un texte non vide, comme `Composer.svelte`).
- Zone de saisie (`ui/messagerie/saisie.slint`, global `DonneesSaisie` ; `src/messagerie/saisie.rs`, `Saisie` et la machine pure `EtatSaisie`) : `TextInput` sur plusieurs lignes,
  qui grandit jusqu'à 220 px puis défile ; Entrée envoie, Maj+Entrée va à la ligne (rien n'est intercepté pendant une composition de la méthode de saisie, `preedit-text`) ;
  compteur au-delà de 3 500 caractères, en rouge à partir de 3 900 (limite de 4 000). Bandeau « Réponse à … » (`repondre_a`) et bandeau de modification (`modifier_message`),
  annulables par Échap ou la croix ; flèche haut dans un champ vide modifie mon dernier message (`Store::dernier_de_moi`). Un texte inchangé n'est pas renvoyé.
- Lecture seule : conversation Estompe, ou salon sans `send_messages` (« Tu n'as pas la permission d'écrire dans ce salon. »), recalculé à chaque rafraîchissement ; hors ligne
  (bandeau de la coquille), une ligne l'annonce au-dessus du champ et les messages attendent.
- Brouillons (`estompe_client::brouillons`) : un texte par conversation (`c:<salon>`, `d:<conversation>`), gardé en quittant la conversation, écrit dans `<données>/brouillons.json`
  300 ms après la dernière frappe (écriture atomique) et à la fin de la session, relu au lancement ; un fichier illisible donne des brouillons vides. Le texte d'une modification
  n'écrase pas le brouillon. Une conversation avec un brouillon porte un crayon dans le panneau (`brouillon` de `LigneSalon` et `LignePrive`).
- Écarts : pas de correction orthographique (le `TextInput` de Slint 1.18 n'en a pas, le front web a celle du navigateur) ; la méthode de saisie et les touches mortes passent par
  winit, à vérifier sur machine réelle.

### Mentions et emoji à la saisie

- Autocomplétion (`estompe_client::autocompletion`, sans interface) : `@` propose les personnes de la conversation puis `@everyone`, `@here` (avec le nombre de personnes
  touchées) et les rôles si `mention_everyone` est permis ; `#` les salons texte du serveur ; `:` suivi de deux caractères les smileys (`emoji::rechercher`, 8 au plus), précédés
  des emoji de serveur dont le nom commence par la saisie. Même tri que `suggest` du front (débuts, puis contenus, 6 au plus), soi-même exclu. 5 000 membres : environ 1 ms (test mesuré).
- Clavier dans le champ (`ui/messagerie/saisie.slint`, `ListePropositions` au-dessus du champ) : flèches haut et bas (en boucle), Entrée ou Tab pour choisir, Échap pour fermer ;
  la liste revient au prochain déplacement du curseur. Le curseur vient du `TextInput` (`cursor-position-byte-offset`, mis à jour avant `edited`) ; Rust le replace par
  `curseur-demande` et `demande-curseur` (`Saisie::placer`).
- Conversion (`convertir_code`) : `:nom:` devient le smiley dès le second `:`, sauf dans du code ou une URL, et sauf si un emoji de serveur porte ce nom (il reste en code, affiché en image).
- Sélecteur (`ui/messagerie/selecteur.slint`, global `DonneesSelecteur` ; `src/messagerie/selecteur.rs`, machine pure `EtatSelecteur` et `Selecteur`) : bouton « Emojis » du champ
  (atteignable par Tab), recherche (nom français, code avec ou sans `:`, mot-clé), récents (32 au plus, `emoji_recents` des préférences), neuf catégories, « Smileys des serveurs »
  (images chargées à l'ouverture), nom et code en pied. Grille de 8 colonnes en rangées de 36 px dans une `ListeVirtuelle` : seules les rangées visibles sont instanciées et leurs
  images rendues (`ModeleRangees`, modèle paresseux). Clavier : la recherche garde le focus ; flèches dans la grille (haut sur la première ligne : retour à la recherche),
  Entrée insère et ferme, Maj+Entrée ou Maj+clic insère sans fermer, Tab passe aux catégories (flèches, Entrée), Échap ferme et rend le focus au champ.
- Cible du sélecteur (`Cible`) : la saisie (insertion au curseur, sélection remplacée) ou une réaction (`Cible::Reaction`, ouverte par « Ajouter une réaction »
  d'un message, voir [Actions sur un message](#actions-sur-un-message)) ; une réaction choisie compte dans les récents.
- Écarts : pas de teinte de peau (le front n'en propose pas non plus) ; `#` (salons) et les emoji de serveur dans la liste de `:` n'existent pas dans le front ; un `#salon`
  inséré reste du texte (le markdown n'a pas de lien de salon).

### Pièces jointes à l'envoi

- Trois entrées, seulement si la conversation accepte l'écriture et `attach_files` : bouton « Joindre un fichier » à gauche du champ (`Fichiers::choisir_fichiers`, sélecteur du
  système par `rfd`, sur un fil à part), dépôt de fichiers sur la fenêtre (évènements `HoveredFile`, `HoveredFileCancelled` et `DroppedFile` de winit, `Coquille::suivi_du_depot` ;
  la zone « Dépose pour joindre · 10 Mo max par fichier » couvre le champ pendant le survol), Ctrl+V dans le champ (`PressePapiers::lire_pieces` : une liste de fichiers, sinon une
  image, collée en `capture.png` ; un texte garde le collage normal du `TextInput`).
- Préparation (`src/messagerie/pieces.rs`, `preparer`, hors du fil d'interface) : nom, type déduit de l'extension (`type_mime_du_nom`), taille ; vignette carrée de 128 px pour une
  image ; une image JPEG, PNG ou WebP plus grande que 1 920 px ou plus lourde que 300 Ko est réduite (`dimensions_a_envoyer`) puis réencodée, gardée si elle est plus légère ; une vidéo est convertie (ci-dessous).
- Conversion d'une vidéo (`estompe_client::video`, appelée par `preparer`), même règle que l'app Tauri et le front (`uploadVideoFile`) : seul un MP4 dont la piste vidéo
  n'est pas en H.264 (`codec_mp4` lit le type d'échantillon du `moov`, en tête ou en fin de fichier ; `hvc1`, `hev1`, `av01`, ou piste introuvable) est réencodé, en MP4
  H.264 `yuv420p` avec son AAC et `moov` en tête (`+faststart`), au débit de l'original × 1,4 borné entre 2 et 12 Mbit/s (`debit_kbps`). Un MP4 déjà en H.264, un WebM ou tout
  autre fichier part tel quel. Encodeurs essayés dans l'ordre, seulement s'ils sont dans `ffmpeg -encoders` : `h264_nvenc`, `h264_amf`, `h264_qsv`, `h264_mf` (comme l'app
  Tauri, aucun encodeur logiciel : le ffmpeg LGPL embarqué n'a pas `libx264`) ; sans ffmpeg, sans encodeur ou si tous échouent, l'original part (le front fait de même).
  La limite `max_video_size` s'applique au fichier converti.
- ffmpeg (`ffmpeg_du_systeme`) : `ffmpeg` (`ffmpeg.exe`) à côté de l'exécutable, sinon celui du `PATH` ; même code sous Linux et Windows, lancé par
  `estompe_plateforme::commande_d_arriere_plan` (sous Windows, sans fenêtre de console et en priorité basse). Sous Windows, l'installeur
  (`installeur/estompe-natif.nsi`) le copie à côté d'`estompe-natif.exe` : c'est le binaire de l'app Tauri, téléchargé par
  `apps/estompe/windows/scripts/fetch-ffmpeg.mjs` (build `win64-lgpl` de [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds), autobuild épinglé
  `autobuild-2026-11-09-12-42`, FFmpeg `N-128718-gb2b543c079`, archive vérifiée par sa somme SHA-256 épinglée dans le script), sous licence LGPL version 3 ou
  ultérieure (`--enable-version3`, sans `libx264`), cité dans « À propos » : 130 Mo installé, 41 Mo dans l'installeur compressé en LZMA. Sous Linux, celui du `PATH`.
  La détection n'a lieu qu'à la première vidéo, hors du fil d'interface.
- Pendant la conversion, la pièce affiche « conversion… 42 % » et sa barre (`EtatPiece::Conversion`, lu dans `-progress pipe:1` rapporté à la durée de `ffmpeg -i`). La croix
  l'annule : la tâche interrompue lève le drapeau d'annulation, ffmpeg est tué puis attendu, et le fichier de sortie supprimé. Le fichier converti est écrit dans
  `<cache>/conversions/` (`FichierTemporaire`, supprimé quand la dernière copie de la source de la pièce disparaît : pièce retirée ou annulée, message envoyé) ; « Réessayer » renvoie ce même fichier. Le dossier est vidé au lancement, pour ce qu'un arrêt brutal aurait laissé.
- Limites (`fichiers::Limites`, `refus_de_piece`, `refus_du_nombre`) : 10 Mo par fichier, `max_video_size` de `GET /me` pour un MP4 (envoyé en `kind=video`), 10 pièces par
  message. Un refus s'affiche sous le champ avec la limite (« Fichier trop gros : « gros.bin » fait 11.0 Mo, 10 Mo maximum. ») et rien ne part.
- Envoi (`televersement.rs` de la crate client, `Client::televerser_en_flux`, `Client::envoyer_piece`) : corps multipart produit par morceaux de 64 Ko lus sur le disque par un fil
  bloquant, quatre morceaux en vol au plus, `Content-Length` exact, un seul nouvel essai sur un 401. Un fichier de 500 Mo ne fait grandir la mémoire résidente que de 2,4 Mo
  (`mesure_memoire_d_un_envoi_de_500_mo`, ignoré par défaut : `cargo test -p estompe-client --release mesure_memoire -- --ignored --nocapture`).
- État d'une pièce (`estompe_client::pieces::ListePieces`, une liste par conversation dans `EtatSaisie`, une pour le message modifié) : préparation, envoi (pourcentage et barre),
  jointe (taille), échec (raison du serveur et « Réessayer », qui relance l'envoi depuis la source gardée). La croix retire la pièce ; pendant l'envoi, elle l'annule : la tâche est
  interrompue et la connexion fermée. Entrée attend que toutes les pièces soient jointes (« Attends la fin de l'envoi des fichiers. ») et refuse une pièce en échec ; un message
  peut partir sans texte. Les pièces suivent leur conversation (envoi poursuivi en changeant de salon) mais ne survivent pas au redémarrage.
- Modification : flèche haut ou « Modifier » reprend les pièces du message (vignette de la liste si elle est chargée) ; retirer ou ajouter change la liste, envoyée en
  `file_ids` seulement si elle diffère.
- Écarts : pas de marquage « masqué » (le front ne le propose pas pour une pièce jointe) ; pas de réordonnancement des pièces d'un message modifié (glisser dans le front) ; une
  image JPEG réduite reste en JPEG (qualité 82) et une PNG ou WebP devient du WebP sans perte, le front produit du WebP avec perte (l'encodeur WebP de `image` est sans perte) ;
  pas de miniature pour une vidéo MP4 (le front en calcule une dans le navigateur) ; sous Linux, seuls `h264_nvenc` et `h264_qsv` peuvent convertir une vidéo (VAAPI
  demanderait un filtre propre à l'appareil) ; dépôt de fichiers non pris en charge par winit sous Wayland, collage par le presse-papiers
  X11 d'`arboard` (XWayland) : à vérifier sur machine réelle.

### Actions sur un message

- Droits (`src/messagerie/actions.rs`, pur) : `droits_du_message` part des droits de la conversation (`DroitsConversation` : écrire, `manage_messages`, `add_reactions`,
  calculés par `PontMessagerie::droits_conversation`) et du message, comme `MessageItem.svelte` : répondre si l'on peut écrire ; modifier son propre message (pas un sondage) ;
  épingler et supprimer son message ou, avec `manage_messages`, celui d'un autre ; réagir avec `add_reactions` ; rien sur un message système ni sur un envoi local. Ils sont
  projetés dans `LigneMessage.droits` (`DroitsLigne`) et `entrees_du_message` en tire le menu.
- Barre au survol (`BarreActions`, `ui/messagerie/element.slint`) : trois réactions rapides (👍, ❤️, 😂, en images), répondre, ajouter une réaction, modifier, épingler ou
  désépingler, copier le message, supprimer, « Plus d'actions » (menu sous le bouton). Placée à cheval sur le haut de la ligne (16 px au-dessus, comme le front), elle reste affichée tant qu'elle est survolée.
- Menu du message (clic droit, touche Menu ou Maj+F10 sur le message qui a le focus) : répondre, modifier, épingler ou désépingler, ajouter une réaction, copier le texte,
  copier le lien (`https://<serveur>/#/s/<serveur>/<salon>/<message>` ou `#/dm/<conversation>/<message>`), marquer comme non lu, « Signaler » (« bientôt », lot 8),
  supprimer (après confirmation « Supprimer ce message ? »). `PontMessagerie::action_message` exécute l'entrée choisie (`src/messagerie/pont/actions.rs`).
- Réactions : un clic ou Entrée sur une réaction (focusable si `add_reactions`) l'ajoute ou la retire ; un bouton après les réactions ouvre le sélecteur.
  `Store::basculer_reaction` applique le changement tout de suite (même chemin que `reaction.updated`, qui ne compte pas deux fois la mienne) puis appelle
  `PUT` ou `DELETE /messages/{id}/reactions/{emoji}` ; un refus remet la réaction comme avant et s'affiche sous le champ.
- Épingler (`Store::epingler`) attend le `message.updated` du serveur. Marquer comme non lu (`Store::marquer_non_lu`) est local : la conversation reprend comme non-lus les
  messages des autres à partir de celui choisi, le séparateur « Nouveaux messages » s'y place, et la lecture n'est signalée qu'au prochain retour en bas de la liste ou à la
  prochaine ouverture (le store passe `en_bas` à faux). L'API ne sait pas reculer la lecture : un redémarrage ou un autre appareil ne voient pas ce non-lu.
- Écarts : la barre n'a pas de position « dessous » pour le premier message visible (elle peut y être coupée par le haut de la liste) ; « Copier le lien » et « Signaler » n'existent pas dans le front ;
  pas de « Copier la sélection » ni « Sélectionner le texte » (Slint ne sélectionne pas un `Text`) ; le sélecteur d'une réaction s'ouvre au-dessus du champ, pas près du message.

### Sondages

- Création (`ui/messagerie/creation-sondage.slint`, global `DonneesCreationSondage` ; `src/messagerie/pont/sondages.rs`) : bouton « Créer un sondage » de la zone de saisie,
  affiché avec la permission `create_polls` (`PontMessagerie::sondage_possible`). Modal « Nouveau sondage » comme `PollModal.svelte` : question, 2 à 10 choix (ajout et retrait),
  « Autoriser plusieurs réponses », clôture 1 h, 24 h (par défaut), 3 jours, 1 semaine, sans clôture ou personnalisée. Slint n'a pas de sélecteur de date : la date
  personnalisée se tape `JJ/MM/AAAA HH:MM`, en heure locale (`sondage::date_saisie`, date impossible refusée).
- Validation (`estompe_client::sondage::valider`, pure et testée) : les règles du front (question non vide, au moins deux choix remplis, choix vides ignorés, échéance future)
  et les limites que le front pose par `maxlength` (500 caractères pour la question, 100 par choix, 10 choix). `TextInput` n'ayant pas de longueur maximale, elles deviennent des
  messages d'erreur, affichés sous le formulaire au premier « Publier le sondage » puis tenus à jour pendant la saisie.
- Publication : le sondage part par la file d'envoi des messages (`Brouillon.sondage`), avec « Envoi… », « Non envoyé » et « Réessayer » ; l'envoi local affiche déjà la carte
  avec ses choix (`EnvoiLocal::message`), sans vote possible.
- Voter (clic ou Entrée sur un choix, focusable tant que le sondage est ouvert) : `Store::voter` applique le vote tout de suite par le même chemin que `poll.voted`
  (`choix_apres_vote`, `comptes_avec` : recliquer retire son vote, en choix unique le nouveau remplace l'ancien) puis appelle `PUT` ou `DELETE /messages/{id}/vote` ; l'évènement
  du serveur remplace ensuite les totaux. Un refus remet mes choix d'avant (sans effacer les votes des autres arrivés entre-temps) et s'affiche sous le champ.
- « Clore » et la corbeille (lanceur, `manage_messages` : mêmes droits que la suppression du message) : `Store::clore_sondage` (`POST /messages/{id}/poll/close`, clôture
  appliquée sans attendre `poll.closed`) ; suppression après la confirmation « Supprimer ce sondage ? ». « Voir les votants » (dès un vote) demande
  `GET /messages/{id}/poll/votes` et affiche les pseudos sous chaque choix jusqu'à « Masquer les votants » (`PontMessagerie::afficher_votants`) ; la liste n'est pas relue à chaque vote, comme dans le front.

### GIF

- Sélecteur (`ui/messagerie/gifs.slint`, global `DonneesGifs` ; `src/messagerie/gifs.rs`, `src/messagerie/pont/gifs.rs`) : bouton « GIF » de la zone de saisie, avant « Emojis »
  (atteignable par Tab), affiché si le bloc `gifs` est actif et la capacité `estompe:gifs.use` accordée (`Etat::capacite`, même règle que `autorise` du front), hors lecture seule.
  Ouvrir l'un des deux sélecteurs ferme l'autre. Ouverture sur les tendances (`GET /gifs/trending`), recherche (`GET /gifs/search`) après 350 ms sans frappe (`slint::Timer`
  relancé à chaque touche, `TEMPORISATION`), « Tendances » et huit idées de recherche en pastilles (lancées tout de suite). Logique sans interface dans `estompe_client::gifs`
  (`RechercheGifs`, testée) : une réponse d'une recherche remplacée est ignorée, la suite sans doublon, arrêt sur une page vide ou au-delà du décalage 480 (le serveur repartirait de 0).
- Grille : rangées justifiées de deux GIF (même hauteur, largeurs selon leur rapport borné entre 0,4 et 2,5, 200 px de haut au plus) dans une `ListeVirtuelle` ; suite chargée à
  160 px du bas (`MARGE_DE_SUITE`). Aperçus `preview` (Giphy `fixed_width_small`) chargés depuis giphy.com par `Medias` (`Source::Externe`, `Format::Gif` : 200 × 200 au plus,
  coins arrondis dans les pixels, toutes les images de l'animation décodées hors du fil d'interface).
- Animation : une minuterie pour le sélecteur, armée seulement tant qu'un GIF animé est **à l'écran** (`rangees_visibles` sur la position de défilement) ; un GIF qui sort
  de l'écran s'arrête, la minuterie aussi quand plus aucun ne bouge ou à la fermeture. Une image change par `set_row_data` du modèle de cellules de sa rangée.
- Clavier : la recherche garde le focus ; flèches dans la grille (haut sur la première rangée : retour à la recherche), Entrée envoie le GIF actif, Tab passe aux idées
  (flèches, Entrée), Échap ou un clic à côté ferme et rend le focus au champ.
- Envoi : `POST /gifs/import` (`Client::importer_gif`) puis message sans texte avec la pièce (`Brouillon.pieces`) par la file d'envoi, en réponse au message cité s'il y en a un
  (le bandeau se ferme) ; un échec s'affiche sous le champ (« Le GIF n'a pas pu être envoyé : … »). Attribution « Propulsé par GIPHY · les aperçus sont chargés depuis giphy.com »
  en pied, comme le front.
- Erreurs : 501 « La recherche de GIF n'est pas activée sur ce serveur. », 429 « Trop de recherches de GIF en peu de temps : réessaie dans une minute. », autres « Recherche de GIF
  impossible. » ; après une erreur la suite n'est plus demandée. La recherche passe `OptionsAppel::sans_relance` : le client ne relance pas un `GET` refusé par 429 (sans
  cette option il attend `Retry-After` et réessaie une fois).
- Écarts : rangées justifiées au lieu de deux colonnes en maçonnerie (un GIF très large ou très haut est recadré) ; indicateur de chargement au lieu des cases grisées animées ;
  idées sur deux lignes fixes (Slint 1.18 n'a pas de retour à la ligne automatique d'une rangée) ; le message 429 est propre à l'app native (le front affiche « Recherche de GIF impossible. »).

### Messages vocaux

- Logique sans interface dans `estompe_client::message_vocal` (testée sans périphérique, sur des signaux de synthèse) : niveau en dBFS et forme d'onde de 40 barres
  (`niveau`, `forme_d_onde`, mêmes calculs que `levelOf` et `toWaveform` du front), rééchantillonnage linéaire (`audio::Reechantillonneur`, le même que le vocal), encodage Opus (`Encodeur` : 48 kHz mono,
  32 kbit/s comme `recorder.svelte.ts`, application VoIP, trames de 20 ms) dans un Ogg écrit à la main (`OpusHead`, `OpusTags`, une page par seconde, granule exact
  à la fin), décodage d'un Ogg Opus ou d'un WebM Opus (lecture EBML minimale, tailles inconnues de MediaRecorder comprises) ; tout autre format (AAC en MP4 de
  Safari) passe par ffmpeg s'il est présent (`decoder_avec_ffmpeg`), sinon le message est « illisible ». Opus vient de `unsafe-libopus` (libopus 1.3.1 traduit en
  Rust : aucune chaîne C à installer, ni sous Linux ni sous MSVC).
- Fichier produit : `message-vocal.ogg` (`audio/ogg`, Opus), que le serveur accepte (`application/ogg`) et que lisent Chrome, Firefox, Safari 17 et la WebView
  d'Android ; envoyé par `Client::envoyer_vocal` (`kind=voice`, `duration_ms`, `waveform` avant le fichier) puis en message sans texte par la file d'envoi, en
  réponse au message cité s'il y en a un. Formats de test versionnés dans `apps/estompe/crates/client/src/message_vocal/essais/` : `chrome.webm` (WebM à taille
  inconnue), `firefox.ogg`, `safari.m4a` (MP4 fragmenté, AAC), produits par ffmpeg comme les produirait MediaRecorder, et `natif.ogg`, produit par l'app ;
  les tests décodent les uns et font relire à ffmpeg (autre décodeur, celui de Chromium) ceux de l'app.
- Son : traits `Micro` et `SortieSon` de la crate plateforme (flux mono avec sa fréquence, fermé quand son `FluxSon` disparaît), implémentés par
  `SonPeripheriques` sur les mêmes périphériques que le vocal (`SystemeAudio`, `AudioSysteme`, `cpal` : WASAPI sous Windows, ALSA sous Linux) : micro et sortie
  choisis dans les réglages audio (`micro`, `sortie_audio` des préférences, relus à chaque ouverture), périphérique par défaut si le choix a disparu ; plusieurs
  canaux ramenés à un seul. Un refus d'accès (`PermissionDenied`) donne « Accès au micro refusé… », l'absence de micro « Aucun micro disponible… », toute autre
  erreur « Le micro est inutilisable. », sous le champ.
- Enregistrement (`ui/messagerie/saisie.slint`, global `DonneesMessageVocal`) : le bouton « Envoyer » devient « Message vocal » quand le champ est vide, sans pièce ni
  modification en cours, si la conversation accepte les fichiers et qu'un micro existe. Le micro rend ses échantillons sur son fil ; un fil `estompe-vocal`
  rééchantillonne à 48 kHz, encode, calcule un niveau toutes les 100 ms et prévient l'interface (durée et 40 derniers niveaux). La durée est celle des
  échantillons reçus, pas d'une horloge ; l'enregistrement s'arrête seul à 5 minutes (`DUREE_MAX_MS`). « Mettre en pause et écouter » ferme le micro et passe
  à l'écoute (lecteur de l'aperçu, forme d'onde, envoi) ; « Envoyer » pendant l'enregistrement enchaîne arrêt et envoi ; la corbeille annule. Un échec d'envoi
  laisse l'écoute avec « Envoi impossible. » et « Réessayer ». Changer de conversation annule l'enregistrement.
- Lecture (`MessageVocal`, `ui/messagerie/vocal.slint`) : bouton lecture et pause, forme d'onde du message avec la position (un clic s'y place), temps écoulé
  pendant la lecture et durée sinon, vitesse 1×, 1,5×, 2× (`Lecture` : WSOLA, la hauteur ne change pas), point « non écouté » (500 derniers messages écoutés
  dans `vocaux_ecoutes` des préférences), téléchargement comme un fichier. Le fichier est demandé à la première lecture et décodé hors du fil d'interface
  (8 messages décodés gardés) ; un seul message joue à la fois (`Lecteur`), la position est suivie toutes les 100 ms par un `slint::Timer` qui ne tourne que
  pendant la lecture ; à la fin, retour au début.
- Écarts : pas d'enchaînement automatique sur le message vocal suivant, une vitesse commune à tous les messages, position oubliée quand un autre message est
  lancé ; pas de geste « maintenir pour enregistrer » (tactile) ; pas d'annulation d'écho ni de réduction de bruit à l'enregistrement (le navigateur les fait ;
  la chaîne du micro du vocal n'y est pas branchée) ; pas de transcription (lot 9).
- À vérifier sur une vraie machine (Windows et Linux) : micro et haut-parleurs réels, refus du micro par les réglages de confidentialité de Windows, message
  enregistré lu par le front web et l'inverse.

## Vocal

Socle sans interface du lot 4 ([#276](https://github.com/Plblsfr/plbls.fr/issues/276)) : SDK Rust LiveKit, périphériques audio, session vocale,
chaîne du micro (gain, suppression de bruit, annulation d'écho, détection de voix, test du micro), écoute (volume et muet par participant, volume général, limiteur),
interface du salon vocal et appuyer pour parler par raccourci global.

### Emplacement et décision

Le SDK (`livekit` 0.12, libwebrtc précompilé) n'impose aucune dépendance d'interface : le vocal est un module de la crate client, `apps/estompe/crates/client/src/vocal/`,
derrière la feature `livekit` de `estompe-client`. L'app l'active par défaut (feature `livekit` de `estompe-natif`) ; sans elle, `ConnecteurIndisponible` refuse toute connexion.
Le reste du module (état, session, mixage, faux serveur) compile et se teste sans le SDK. Les périphériques passent par `cpal`, dans la crate plateforme.

| Fichier | Rôle |
|---|---|
| `crates/plateforme/src/audio.rs` | `AudioSysteme` (trait `SystemeAudio`) : liste des entrées et sorties, périphérique par défaut, ouverture d'un flux d'entrée ou de sortie en `f32` entrelacé ; débranchement signalé par `ErreurFluxAudio::Debranche` ; `SonPeripheriques` (traits `Micro`, `SortieSon`) : les mêmes flux en mono pour les messages vocaux |
| `crates/client/src/audio/format.rs` | format de LiveKit (48 kHz mono, trames de 10 ms, 480 échantillons `i16`) : passage en mono, rééchantillonnage linéaire sans dérive (pas entiers), découpe en trames (`ConversionEntree`), conversion vers la sortie (`ConversionSortie`), tampon de lecture amorcé et borné (`TamponLecture`) |
| `crates/client/src/audio/peripheriques.rs` | `Capture` et `Lecture` : périphérique choisi, sinon celui par défaut ; changement à chaud (`choisir`) ; `surveiller` bascule sur celui par défaut quand le choisi est débranché et y revient quand il réapparaît (`Bascule`) |
| `crates/client/src/audio/micro.rs` | chaîne du micro, pure et sans allocation dans la boucle audio : `ReglagesMicro` (bruit, gain, détection de voix, seuil), `ChaineMicro`, `DetectionVoix`, `niveau_db` (équivalent de `rmsDb`), `MesureMicro` (niveau lu par l'interface), `Apm` (traitement de WebRTC partagé entre micro et sortie) |
| `crates/client/src/audio/essai.rs` | `EssaiMicro` : test du micro hors appel (niveau en direct, retour de sa voix traitée) |
| `crates/client/src/audio/factice.rs` | `SystemeFactice` : périphériques factices pilotés par les tests (`brancher`, `debrancher`, `pousser`, `tirer`) |
| `crates/client/src/vocal/salle.rs` | traits `Connecteur`, `Salle`, `SortieMicro` et `EvenementSalle` : ce que la session attend d'une salle, LiveKit ou fausse |
| `crates/client/src/vocal/livekit.rs` | `ConnecteurLiveKit` (feature `livekit`) : connexion, attributs, publication du micro (`NativeAudioSource`), réception des pistes audio (`NativeAudioStream`), traduction des évènements ; `apm()` : `AudioProcessingModule` de WebRTC (écho, gain automatique, passe-haut, bruit) |
| `crates/client/src/vocal/etat.rs` | `EtatVocal`, pur : phase, salon, micro et casque coupés, coupures imposées, porte du micro, micro indisponible, participants (attributs, qualité), qui parle ; `etats_affiches` (équivalent de `voiceStates`) |
| `crates/client/src/vocal/session.rs` | `SessionVocale` : acteur sur un runtime tokio, commandes, reconnexion, audio |
| `crates/client/src/vocal/mixage.rs` | `Ecoute` (volumes et muets par participant, voix et son du partage à part, volume général ; équivalent de `gainOf`), `Limiteur`, `Mixeur` : une file par participant et par source, gain précalculé, sourdine, référence d'écho |
| `crates/client/src/vocal/erreurs.rs` | `ErreurVocal` (salon plein, droit manquant…) et `CauseMicro` (équivalents de `micCause` et `micError`) |
| `crates/client/src/vocal/factice.rs` | `ServeurVocalFactice` (connecteur et salles en mémoire, coupure du réseau, exclusion) et `ApiVocaleFactice` |
| `crates/client/src/vocal/scene.rs` | scène vidéo sans interface, équivalent de `stage.ts` du front : `construire_elements` (`buildItems`), `dimensions_grille` (`gridDims`), `doit_recevoir` (`shouldSubscribe`), `rassembler_spectateurs` (`collectWatchers`), `Epinglage` (tuile mise en avant, épinglée ou détachée, comme `VoiceChannelView.svelte`), `QualiteRecue` (qualité reçue d'un partage) |
| `crates/client/src/vocal/video.rs` | vidéo sans interface : `CleVideo` (propriétaire et source, caméra ou partage), trait `TrameVideo` (image décodée convertie en RGBA à la taille voulue), `ImageI420` (conversions BT.601, réduction, formats YUYV, NV12, RGB et gris), `Mire` (piste factice des tests et des captures), `RecepteurVideo` (dernière image seulement, compteur d'images reçues), `Videos` (un récepteur par flux) |
| `crates/plateforme/src/camera.rs` | trait `SystemeCamera` (liste des caméras, ouverture d'un flux avec la résolution et la cadence voulues, images rendues telles que la caméra les donne : MJPEG, YUYV, NV12, gris, RGB, BGR), `ErreurCamera` (absente, introuvable, occupée, refusée, débranchée), `CamerasSysteme` (nokhwa : V4L2 sous Linux, Media Foundation sous Windows, un fil par caméra ouverte) |
| `crates/client/src/vocal/camera.rs` | `QualiteCamera` (360p, 720p, 1080p, mêmes résolutions et débits que `CAMERA` du front), `image_i420` (MJPEG décodé par zune-jpeg), `CaptureCamera`, `ApercuCamera`, `message_camera`, `CamerasFactices` (caméras des tests : brancher, occuper, débrancher, pousser des images de la mire) |
| `crates/client/src/vocal/ptt.rs` | `PousserPourParler` : appuyer pour parler (touche de la fenêtre et raccourci global, délai de relâchement), `ReglagesPtt` |
| `crates/client/src/audio/tonalite.rs` | sons d'arrivée et de départ (mêmes notes que le front), joués sur la sortie choisie |
| `crates/plateforme/src/raccourcis/` | trait `RaccourcisGlobaux` : crochet clavier bas niveau sous Windows, XInput 2 sous X11, `RaccourcisFactices` pour les tests |
| `natif/src/vocal/` | `projection.rs` (pur : tuiles, participants du panneau, vue, barre, menu d'un participant), `pont.rs` (`PontVocal` : état de la session vers `DonneesVocal`, commandes, réglages, test du micro, sons), `touches.rs` (touche Slint vers code de touche) |
| `natif/src/video/` | affichage des flux, réutilisable par toute vue vidéo : `Afficheur` (un fil par flux affiché, conversion hors du fil d'interface), `GestionVideo` (flux voulus, modèle `DonneesVideo.flux`), `convertir` |
| `natif/ui/video/flux.slint` | `DonneesVideo` (images des flux, taille d'une tuile) et `ImageFlux` (image d'un flux, recadrée ou entière, coins arrondis) |
| `natif/ui/vocal/camera.slint` | fenêtre « Activer la caméra » : aperçu en miroir, erreur, choix de la caméra et de la qualité |
| `natif/ui/vocal/` | vue du salon (`vue.slint`), tuile et ligne du panneau (`tuile.slint`), commandes (`commandes.slint`), barre du panneau (`barre.slint`), menus (`menus.slint`), réglages audio (`reglages.slint`) |

### Session vocale

`SessionVocale::demarrer(runtime, Dependances { api, connecteur, audio, apm, reglages })` lance un acteur sur le runtime donné (dans l'app : celui de l'entrée,
`Application::vocal`). Commandes : `rejoindre`, `deplacer` (sans coupure : l'ancienne salle n'est quittée qu'une fois la nouvelle connectée), `deplace_par_moderation(de, vers)`
(évènement `voice.moved`, accepté si l'on est dans `de` ou si on l'a quitté depuis moins de 5 s), `quitter`, `basculer_micro`, `basculer_casque`, `imposer(micro, son)`
(`mic_off`, `sound_off` d'un modérateur), `definir_porte(ouverte)` (appuyer pour parler ; la détection de voix est dans la chaîne du micro), `choisir_micro`, `choisir_sortie`,
`definir_traitement(ReglagesMicro)`, `definir_volume(identité, source, pourcent)`, `definir_muet(identité, source, muet)`, `definir_volume_general(pourcent)`, `fermer().await` ou `fermer_bloquant(délai)` (sortie propre, étape `Vocal` de la fermeture de l'app, après l'arrêt de la coquille).
L'état se lit par `etat()` (`watch` de `EtatVocal`, qui porte aussi `traitement` et `ecoute`) ; `niveau_micro()` lit sans attendre le niveau du micro et la voix détectée ; `abonner_avis()` donne les `AvisVocal` (rejoint, quitté, arrivée, départ, déplacé, bascule de périphérique).

- Un seul vocal à la fois : l'identité LiveKit est l'utilisateur ; rejoindre ailleurs (app Tauri, web) déconnecte avec `DuplicateIdentity`, la session revient à « inactif » sans se reconnecter.
- Jeton : `Client::jeton_vocal` (`POST /voice/{salon}/token`) ; `voice_full`, 403, `not_voice_channel`, 502 deviennent un message (`ErreurVocal::message`), la session reste inactive.
- Micro : `Capture` → trames de 10 ms → `ChaineMicro` (toujours, pour mesurer le niveau même micro coupé) → `SortieMicro` (piste `microphone`, DTX et RED) seulement si `EtatVocal::micro_emet()` ; sinon la piste est coupée (`mute`).
  Micro introuvable ou refusé : connecté en écoute seule (`micro_indisponible`, message suivi de « Vous êtes en écoute seule. »).
- Écoute : chaque piste audio reçue alimente le `Mixeur`, lu par `Lecture` sur la sortie choisie ; casque coupé ou `sound_off` : plus rien n'est joué.
- Coupé par un modérateur (`imposer`, ou permission de publier retirée par le serveur) : le micro ne se réactive pas tant que la coupure dure.
- Attributs publiés à chaque changement et après une reconnexion : `muted`, `deaf` (mêmes clés que le front).
- Reconnexion : le SDK reprend seul une coupure courte (`Reconnexion`, `Reconnecte`, jusqu'à 60 s). S'il abandonne, la session redemande un jeton et se reconnecte
  (1, 2, 4, 8 puis 10 s, 12 essais) en gardant micro et casque ; une déconnexion voulue par le serveur (exclusion, salle fermée, doublon d'identité) ne se reconnecte pas.
- Fils : l'acteur et les tâches de LiveKit sur le runtime ; capture et lecture sur les fils de `cpal`, qui ne touchent que des verrous courts (`RouteMicro`, `Mixeur`) ; libwebrtc a ses propres fils.

### Chaîne du micro

Mêmes réglages et valeurs par défaut que le front (`prefs.noise`, `micGain`, `vad`, `vadThreshold`), retenus dans `traitement_micro` :

| Réglage | Front | Natif |
|---|---|---|
| Bruit `aucun` | contraintes du navigateur coupées | rien |
| Bruit `standard` | `echoCancellation`, `noiseSuppression`, `autoGainControl` du navigateur | `AudioProcessingModule` de WebRTC (écho, gain automatique, passe-haut, bruit), référence d'écho = sortie du `Mixeur` |
| Bruit `renforce` (défaut) | idem + RNNoise (WASM) | idem + `nnnoiseless` (RNNoise en Rust, trames de 480 échantillons) |
| Gain | 0 à 300 % | idem, après la mesure du niveau |
| Détection de voix | seuil de -75 à -15 dB (défaut -45), maintien 400 ms | idem, mesurée à chaque trame de 10 ms ; porte fermée : la piste envoie du silence (rampe de 1 ms à l'ouverture, 10 ms à la fermeture) |

Ordre : APM, `nnnoiseless`, mesure du niveau (`niveau_db`, publié dans `MesureMicro`), détection de voix, gain, porte. Le traitement tourne sur le fil de capture de `cpal`,
dans des tampons fixes alloués à la création (un changement de réglage se fait sur le fil de l'acteur, sous le verrou de `RouteMicro`). L'APM existe seulement avec la feature `livekit`.
`porte_ouverte(ptt, vad, touche, voix)` reprend `gateOpen` : l'appuyer pour parler l'emporte sur la détection de voix.
Test du micro : `EssaiMicro::ouvrir(audio, micro, sortie, reglages, apm)` (hors appel), `niveau()`, `regler`, `ecouter(true)` pour entendre sa voix traitée.

Mesures (tests de `audio::micro`, voix de synthèse versionnée `crates/client/tests/donnees/voix-16k.wav` plus bruit de fond et clics générés) : en mode renforcé,
le bruit hors parole baisse de 26,5 dB (-40,6 → -67,1 dBFS) ; sur 50 ms après chaque début de mot, la voix traitée reste à moins de 1 dB de la voix propre
(RNNoise atténue toutefois les 10 à 20 premières millisecondes). Annulation d'écho (feature `livekit`, écho simulé linéaire, retard de 30 ms) : -52 dB.
Coût mesuré sur une machine de développement (4 cœurs, release) : 1,0 % d'un cœur pour `nnnoiseless` et la détection de voix, 2,7 % avec l'APM et la référence d'écho.

### Écoute

`Ecoute` (préférence `ecoute`) reprend `volumes`, `screenVolumes`, `mutedVoice`, `mutedScreen` du front : volume de 0 à 300 % et muet par participant, la voix
et le son du partage (`<id>#screen`, rangé sous `<id>`) à part ; comme le front, c'est par utilisateur, quel que soit le salon. Volume général du vocal de 0 à 100 %,
qui n'existe pas dans le front. Le muet garde le volume. Gain = 0 si casque coupé ou muet, sinon volume × volume général.
Le `Mixeur` additionne les pistes (gain précalculé à chaque changement, aucune recherche par nom dans la boucle audio), puis un limiteur à attaque immédiate
et relâchement de 100 ms garde le signal sous 0,98 sans écrêtage dur (dix pistes à pleine échelle : sinusoïde conservée). Il n'ajoute aucun échantillon de retard
au tampon de réception (amorce de 20 ms, `TamponLecture`). La sortie mixée sert de référence à l'annulation d'écho.

L'app lit `traitement_micro` et `ecoute` au démarrage (`Reglages`) ; l'interface appelle la session pendant un réglage, et enregistre la préférence au relâchement
(comme `prefs.save()` sur `onchange`), avec les mêmes fonctions (`Ecoute::definir_volume`, `definir_muet`, `definir_general`, `ReglagesMicro::borne`).

### Interface du salon vocal

Global `DonneesVocal` (`ui/vocal/donnees.slint`), rempli par `PontVocal` (`src/vocal/pont.rs`, tenu par le `Pont` de la coquille). La coquille suit l'état de la session
(`SessionVocale::etat`), ses avis (sons d'arrivée et de départ, bascule de périphérique) et, pendant un appel ou un test du micro, lit le niveau du micro toutes les 50 ms
(`PERIODE_MESURE`) ; hors appel, rien ne tourne.

- Panneau : les salons vocaux et cinéma ne sont plus « bientôt » ; un clic ouvre le salon et le rejoint (comme `SidePanel.svelte`). Sous chaque salon, ses participants
  (photo de profil, sinon initiales, avec halo, micro et casque coupés quand on est dans ce salon, coupures du serveur) ; à droite du nom du salon, sa durée depuis
  l'arrivée du plus ancien participant (`since`, comme `oldestSince` et `VoiceTimer.svelte`), rafraîchie chaque seconde tant qu'un salon affiché est occupé. Au-dessus de la barre du compte, la barre du vocal : état
  (« Vocal connecté », « Connexion… », « Reconnexion… »), salon et serveur (clic : ouvre le salon), durée de présence (`since` du serveur, sinon la connexion), qualité, commandes, avis.
- Glisser un participant (comme `SidePanel.svelte`) : seulement sur un membre que l'on domine, avec `move_members`, quand le serveur a un autre salon vocal ou cinéma
  (`Participant::deplacable`, mêmes conditions que `canDrag`). `DragArea` sur la ligne du participant (curseur « saisir », ligne estompée pendant le glisser), `DropArea` sur
  chaque salon vocal (`ElementListe::depot-actif`) : pendant le glisser, les autres salons vocaux sont bordés de rose (`glisse-depuis`, comme `.nav-item.drop`), celui
  survolé se remplit ; le dépôt envoie `PUT /voice/{salon}/participants/{id}/move`, la route de « Déplacer vers ». La donnée glissée (`DataTransfer` de Slint, salon et
  participant dans `user_data`) reste dans la fenêtre. Un refus du serveur s'affiche sous le salon visé (« Action impossible : … », notification dans le front) jusqu'au dépôt suivant.
- Rail : pastille « Connecté en vocal » sur le serveur du vocal. Accueil : « Rejoindre » ou « Ouvrir » sur chaque vocal en cours.
- Vue du salon : hors du salon, l'écran pour rejoindre (participants, « n / 10 participants », erreur du serveur, permission `connect`, réglages audio) ; dans le salon,
  une grille de tuiles (même calcul que `gridDims` : 1, 2×1, 2×2, 3×n, 4×n), « Tu es seul·e ici », bandeau de reconnexion, et en bas les commandes :
  micro (jauge, seuil franchi en rose), choix du micro, casque, choix de la sortie, caméra, partage d'écran, réglages audio, raccrocher. « Ton micro est coupé » quand on parle micro coupé.
- Mode plein cadre (bouton de l'en-tête, comme `VoiceChannelView.svelte`) : rail, panneau, en-tête et marge disparaissent, la fenêtre passe en plein écran, les commandes
  restent en barre flottante avec « Quitter le mode plein cadre » ; Échap ou quitter le salon en sort. Le front n'agrandit une tuile (« Mettre en grand ») que pour un flux
  vidéo : il arrivera avec la caméra et le partage d'écran (lot 5).
- Tuile : photo de profil (même cache et mêmes clés que la liste des membres, `coquille/avatars.rs` : avatar propre au serveur, sinon global ; demandée au premier affichage), sinon initiales, pseudo, « (toi) », halo et onde quand il parle (`EtatVocal::parlent` de LiveKit, et pour soi le niveau du micro au-dessus du seuil), micro et casque coupés
  (attributs `muted`, `deaf` : qui parle n'est jamais affiché muet), coupures du serveur, qualité. Clic ou Entrée : menu du participant au centre de la fenêtre ; clic droit (comme `use:ctx` du front), touche Menu ou Maj+F10 :
  le même menu ouvert au pointeur, ou sous l'élément au clavier, sans voile, gardé dans la fenêtre à 8 px des bords (`Modal::au-pointeur`, `DonneesVocal::ouvrir-menu-a`) ;
  à la fermeture, le focus revient au participant. Contenu : muet pour moi, volume 0 à 300 %
  retenu au relâchement, « Couper le micro / le son du serveur » si l'on domine le membre et a `mute_members` ou `deafen_members`, `PUT /voice/{salon}/participants/{id}/mod` ;
  « Déplacer vers « … » » pour chaque autre salon vocal ou cinéma du serveur avec `move_members` et « Déconnecter du vocal » avec `disconnect_members`, toujours sur un membre
  que l'on domine, comme le menu du panneau du front : `PUT /voice/{salon}/participants/{id}/move` (`channel_id`) et `POST /voice/{salon}/participants/{id}/disconnect`).
  Le menu se ferme quand le serveur accepte ; un refus s'affiche dans le menu (« Action impossible : … »). Le même menu s'ouvre depuis le panneau, au clic, au clic droit et au clavier.
- Clavier : toutes les commandes sont des boutons atteints par Tab ; Alt+Maj+M (micro) et Alt+Maj+D (casque) comme le front ; Échap ferme menus et réglages.
- Modération : `voice.moved` devient `Changement::DeplacementVocal` dans le store puis `deplace_par_moderation` (avis « eve t'a déplacé dans … ») ; la coupure `voice.mod` du compte (store,
  `GET /servers/{id}/voice` puis `voice.mod`) est passée à `SessionVocale::imposer` ; `voice.disconnected` s'affiche dans la barre.
- Réglages audio (modal) : micro et sortie, jauge et test du micro hors appel (`EssaiMicro`), bruit, gain, détection de voix et seuil, appuyer pour parler, volume général,
  sons d'arrivée et de départ. Chaque réglage s'applique pendant le glisser et s'enregistre au relâchement.
- Sons : à soi et aux autres, préférence `sons_vocal` ; choix du son d'arrivée et de départ parmi ceux du front (`SOUND_PRESETS` : Montée 660 puis 880 Hz, Descente,
  Carillon, Pop, Double bip, Aucun ; notes de 100 ms) et volume des sons de 0 à 100 % (60 % par défaut), préférence `sons` (`ReglagesSons`, `audio/tonalite.rs`, mêmes
  identifiants que `joinSound`, `leaveSound`) ; « Écouter » et un nouveau choix jouent le son sur la sortie choisie.
- Son personnalisé (`pickSound` de `SettingsModal.svelte`) : « Mon fichier… » sur chaque son ouvre le sélecteur du système filtré sur les fichiers audio
  (`Fichiers::choisir_fichier` de la crate plateforme, `rfd`, sur un fil à part). Le fichier est refusé au-delà de 150 Ko ou s'il ne se décode pas (« Choisis un fichier
  audio de 150 Ko au plus. », sous les sons) ; sinon il est copié tel quel sur l'appareil (`<données>/sons/arrivee` ou `depart`, `SonsPersonnalises`, au lieu du
  `data:` des préférences du front), devient le son choisi (`custom`, `ChoixSon::Personnalise`) et joue aussitôt en aperçu. « Mon fichier » apparaît alors parmi les sons ;
  un son prédéfini ramène au son par défaut sans effacer le fichier, qui reste proposé. Décodage (`audio/son_perso.rs`) : WAV (PCM 8 à 32 bits, flottant) et Ogg ou WebM Opus
  dans l'app, autres formats (MP3, AAC, FLAC, Vorbis…) par ffmpeg s'il est présent (`ffmpeg_du_systeme`), ramenés en mono 48 kHz ; joué au volume des sons comme
  l'élément `Audio` du front (gain linéaire), décodé une fois puis gardé en mémoire ; un nouvel aperçu coupe le précédent.
- Écarts : le refus d'un fichier s'affiche dans les réglages et celui d'un dépôt sous le salon, pas dans une notification ; le choix du fichier est filtré sur des extensions
  audio (`accept="audio/*"` dans le front) et un fichier que ni l'app ni ffmpeg ne décode est refusé dès le choix (le front le garderait sans pouvoir le jouer).

### Vidéo

Affichage d'une piste vidéo reçue de LiveKit, pensé pour toutes les vues vidéo (tuiles de la scène, flux agrandi et partage d'écran, aperçu d'un partage, incrustation de la caméra).

- Réception : la salle signale les pistes vidéo publiées (`VideoPubliee`, `VideoRetiree`, aussi pour une piste coupée par son auteur) sans s'y abonner. La session
  (`SessionVocale::definir_affichage`) reçoit la liste des flux affichés avec la taille de leur tuile en pixels : elle s'abonne seulement à ceux-là
  (`RemoteTrackPublication::set_subscribed`) et annonce la taille au serveur (`update_video_dimensions`), qui choisit la couche du simulcast (`dynacast`,
  `adaptive_stream`). Une piste hors de la liste est désabonnée : plus rien n'arrive ni n'est décodé. `etat().videos` liste les flux publiés.
- Images : le flux du SDK (`NativeVideoStream`, file d'une image) alimente le `RecepteurVideo` du flux, qui ne garde que la dernière image et compte les images reçues.
  L'image reçue n'est pas convertie sur le fil du vocal : `TrameVideo::vers_rgba` la passe en I420 si besoin, la réduit puis la convertit par libyuv (`I420Buffer::scale`, `to_argb`)
  au moment de l'afficher.
- Affichage (`natif/src/video/`) : un `Afficheur` par flux affiché, sur son propre fil, attend une nouvelle image, la convertit à la taille de la tuile sans
  jamais l'agrandir (`taille_de_conversion`, recadrage comme `object-fit: cover`) dans un `SharedPixelBuffer`, puis la pose dans `DonneesVideo.flux` par la boucle
  de Slint. Une seule image est en route vers l'interface à la fois et la cadence est bornée à 60 images par seconde (`CADENCE_MAX`) : les images qui arrivent
  entre deux affichages sont sautées, jamais mises en file.
- Ce qui est affiché : la tuile d'un participant montre sa caméra quand il en publie une (« · caméra » dans la pastille), sinon sa photo. La scène annonce la taille
  d'une tuile (`DonneesVideo.zone-tuile`). Hors du salon, dans une autre vue, ou fenêtre réduite ou masquée (évènements `Occluded` et `Resized` à 0 de winit,
  `suivi_de_la_visibilite` de la coquille), la liste est vide : abonnements retirés, fils arrêtés.
- Mesure (`video::tests::mesure_une_piste_1080p_a_30_images_par_seconde`, ignorée par défaut, release) : une piste factice 1080p à 30 images par seconde
  affichée dans une tuile de 640 × 360, pendant 10 minutes, sur la machine de développement (4 cœurs) : 18 000 images produites, 18 000 converties, mémoire privée
  stable à 11,4 Mo de la première à la dixième minute, 4,9 % d'un cœur en moyenne (génération de la mire comprise, conversion en Rust pur, sans le rendu Skia ni le décodage
  de libwebrtc). `ESTOMPE_MESURE_VIDEO_S` fixe la durée (30 s par défaut).
- Écarts : la résolution reçue (« · 720p » du front) n'est pas affichée ; Wayland ne signale pas toujours une fenêtre masquée (le décodage continue alors) ;
  le passage des images par la mémoire (`Image::from_rgba8`) plutôt qu'une texture partagée avec Skia, que Slint 1.18 n'expose pas pour une image de modèle.

### Scène vidéo et partages

Disposition de la scène et visionnage d'un partage d'écran d'un autre participant, avec les règles de `stage.ts` et de `VoiceChannelView.svelte` du front.

- Tuiles (`projection::scene`) : une par participant, avec sa caméra ou sa photo, puis une de plus par partage d'écran publié, juste après celle de son auteur
  (`construire_elements`) ; un flux dont l'auteur n'est pas encore listé a quand même sa tuile. Un partage publié par le participant `<id>#screen` de l'app Tauri
  appartient à `<id>`. Grille de `dimensions_grille` (1 à 4 colonnes).
- « Regarder » : la tuile d'un partage non regardé montre la photo de son auteur, l'étiquette « Écran » et « Regarder ». Sans ce choix, la session ne s'abonne ni à
  l'image ni au son du partage (`EtatVocal::regarde`, `doit_recevoir`), même si la tuile est affichée ; « Arrêter de regarder » (outils de la tuile, menu du partage)
  désabonne les deux. Le choix est oublié quand le partage s'arrête. `Reglages::regarder_auto` (faux dans l'app native : un partage ne se regarde que sur demande) garde
  l'équivalent de `watchAuto` du front. Le son de son propre partage n'est jamais rejoué.
- Son du partage : la salle annonce un son de partage (`AudioPubliee`, piste `screen_share_audio` ou piste audio du participant `#screen`) sans s'y abonner ; son volume
  et sa coupure sont propres (`SourceAudio::Partage` de `Ecoute`), réglés dans le menu du partage (clic sur la tuile, bouton du volume, ou au pointeur par clic droit, touche Menu ou Maj+F10, `DonneesVocal::ouvrir-menu-partage-a`) et gardés dans les préférences.
- Mise en avant : un partage regardé passe en grand, les autres tuiles en bande dessous ; « Mettre en grand » épingle une tuile, « Détacher » la remet dans la
  grille (`Epinglage`). Plein écran d'une tuile (double-clic ou bouton) : la fenêtre passe en plein écran avec cette seule tuile, Échap en sort.
- Spectateurs : chaque participant publie dans l'attribut `watching` les pistes qu'il affiche (`EtatVocal::vus`, identifiants de piste LiveKit, comme le front) ;
  `EtatVocal::spectateurs` les regroupe par flux. La tuile en montre le nombre (les noms quand elle est en grand) et le menu du partage la liste.
- Qualité reçue (menu du partage, préférence `qualite_partage`) : « Auto » demande la taille de la tuile (le serveur choisit la couche du simulcast selon la taille et
  la connexion, `adaptive_stream`), « 1080p », « 720p » ou « 360p » demandent cette taille quelle que soit la tuile. La tuile en grand et le plein écran demandent
  leur taille réelle (`DonneesVideo.zone-vedette`), la bande 192 × 108.
- Tests : `scene::tests` (un test par cas de `stage.test.ts`), `vocal::tests::sans_regarder_aucune_image_ni_son_du_partage_n_est_recu`,
  `le_son_d_un_partage_regarde_est_mixe_avec_son_volume_propre`, `regarder_automatiquement_et_ne_jamais_rejouer_son_propre_partage` (serveur factice, mire),
  tests d'interface de `natif/tests/vocal.rs` et captures `vocal-partage-*`.
- Écarts : la résolution reçue (« · 720p ») n'est pas affichée.

### Caméra

Capture par nokhwa dans la crate plateforme (`CamerasSysteme`, seule partie propre à l'OS), conversion en I420 dans la crate client, publication par la session.

- Bouton caméra des commandes (Alt+Maj+V, comme le front) : caméra coupée, il ouvre « Activer la caméra » (`MediaPicker.svelte` et `CameraPreview.svelte`) ; caméra active, il la coupe.
  La fenêtre montre l'aperçu en miroir (« Aperçu en miroir »), la liste des caméras (« Par défaut » puis celles du système), la qualité (360p, 720p recommandé, 1080p, avec le débit montant)
  et « Activer la caméra ». Choix et qualité sont retenus (`camera`, `qualite_camera` des préférences, comme `cameraId` et `cameraQuality`). L'aperçu (`ApercuCamera`) s'ouvre sur
  un fil à part et se ferme avant la publication : une caméra ne s'ouvre pas deux fois.
- Publication (`SessionVocale::activer_camera`) : capture à la qualité choisie, piste `camera` publiée avec le simulcast, le débit et la cadence de la qualité (`TrackPublishOptions` :
  `simulcast`, `video_encoding`), images copiées dans un `I420Buffer` puis `NativeVideoSource::capture_frame`. Ma tuile montre ma caméra en miroir, comme le front.
  Couper la caméra ferme le périphérique (voyant éteint) et retire la piste (`unpublish_track`).
- Changement à chaud (`choisir_camera`) : la capture passe sur l'autre caméra sans retirer la piste ; un changement de qualité relance la caméra (`restartCamera` du front).
- Rejoindre caméra activée (`rejoindre_avec_camera`, comme `join({ camera: true })`) : bouton « Rejoindre avec la caméra » de l'écran pour rejoindre ; la caméra est publiée dès la connexion.
  Une reconnexion la republie ; quitter le salon la coupe.
- Erreurs : caméra absente, introuvable, prise par une autre application, accès refusé ou débranchée pendant l'appel : la caméra revient à « désactivée » et la raison s'affiche
  dans la barre du vocal (`camera_erreur`) ou dans la fenêtre d'aperçu (« Caméra déjà utilisée par une autre application. », « Caméra débranchée. »…).
- Écarts : pas d'effets d'arrière-plan (lot 11) ni de choix du codec (`videoCodec`) ; le message d'un refus d'accès ne distingue pas un refus du système d'une caméra
  occupée quand le pilote ne le dit pas ; sous Linux seules les caméras V4L2 sont proposées (pas de portail caméra de PipeWire).

### Partage d'écran

Sélecteur de source, publication et réglage du partage (`MediaPicker.svelte`, `pickScreen`, `publishScreen`, `changeSource`, `setScreenQuality` du front).
Logique dans la crate client (`vocal/partage.rs` : qualités et plafonds ; `vocal/session/partage.rs` : capture et publication par la session), capture par le trait
`CaptureEcran` de la crate plateforme (voir [Capture d'écran et de fenêtre](#capture-décran-et-de-fenêtre)), interface dans `natif/src/vocal/pont/partage.rs` et `natif/ui/vocal/partage.slint`.

- Identité : le partage est publié par la connexion du vocal (piste `screen_share`), comme le front web, et non par un second participant `<id>#screen`
  comme le prototype de l'app Tauri. Le web, l'app Tauri (qui affiche les pistes avec le code du front) et l'app native le voient chez son auteur, avec ses spectateurs.
- Sélecteur (bouton « Partager l'écran » des commandes, Alt+Maj+S comme le front) : onglets « Écrans » et « Fenêtres », une carte par source avec sa miniature
  (`miniature`, 180 px de haut), son titre et l'application ou la taille ; les miniatures sont recalculées toutes les 2 s tant que la liste est affichée
  (`PERIODE_MINIATURES`), sur un fil à part ; les fenêtres d'Estompe ne sont pas proposées ; « Actualiser », « Annuler », « Aucun écran trouvé. »,
  « Impossible de lister les écrans et fenêtres. ». Sous Wayland ou sans serveur X : « Le partage d'écran n'est pas disponible sur ce système ».
- Aperçu : la source choisie est capturée (au plus 30 images par seconde, `IPS_APERCU_MAX`) et affichée avant de partager, avec son titre et sa taille ; « Changer de source »
  revient à la liste ; une source qui disparaît pendant l'aperçu l'indique.
- Qualité (mêmes valeurs que le front : `SCREEN_RES`, `SCREEN_FPS`, `screenBitrate`, `clampScreen`, `stepDown`, `screenMode`) : préréglages « Fluide » (1080p, 60 i/s,
  fluidité), « Net » (1440p, 30 i/s, netteté), « Économie de données » (720p, 30 i/s) ; résolution (720p à 4K) et images par seconde (30, 60, 120), grisées au-delà
  du plafond du serveur (`screen_max_fps`, `screen_max_height` du jeton vocal, 60 et 2160 par défaut) ou de la hauteur de la source (« (au-delà de ton écran) ») ;
  débit montant estimé et rappels du front ; « Privilégier la fluidité » (indice de contenu et préférence de dégradation de l'encodeur) et « Réduire automatiquement ».
  Réglages retenus (`partage` des préférences : résolution, images par seconde, fluidité, réduction automatique, son).
- Publication (`SessionVocale::partager`) : capture continue de la source (`CaptureEcran::demarrer`, cadence de la qualité, curseur), image RGBA convertie en I420 et
  réduite à la hauteur de la qualité (libyuv par le SDK : `abgr_to_i420`, `I420Buffer::scale`), piste `Screenshare` publiée avec le débit et la cadence de la qualité,
  le simulcast au-delà de 1080p et la dégradation choisie (`TrackPublishOptions::degradation_preference`, source « écran » de WebRTC en mode netteté).
- Pendant le partage : bandeau « Tu partages ton écran » au-dessus des commandes de la scène (nombre de spectateurs, « Qualité », « Changer de source », « Arrêter »),
  rappel permanent « Tu partages : <source> » dans la barre du vocal du panneau, ma tuile de partage (l'image capturée) mise en avant comme un partage regardé.
  Changer de source remplace la capture sans retirer la piste ; changer la qualité règle la cadence de la capture et la hauteur envoyée sans relancer le partage.
- Réduction automatique (`watchScreen` du front) : toutes les 3 s la session lit la raison de limitation de l'encodeur (`get_stats`, `qualityLimitationReason`) ;
  freiné par le débit ou le processeur trois mesures de suite, le partage descend d'un palier (`palier_inferieur`) et l'avis « Connexion ou ordinateur limités : partage
  réduit à … » s'affiche dans la barre ; on ne remonte pas seul.
- Fin : « Arrêter », le bouton des commandes ou Alt+Maj+S retirent la piste et libèrent la capture ; quitter ou changer de salon arrête le partage ; une reconnexion le republie.
  Fenêtre fermée ou écran perdu : « Partage arrêté : la fenêtre partagée a été fermée. » (ou « l'écran partagé n'est plus disponible ») ; fenêtre réduite : rien n'est envoyé jusqu'à sa restauration.
- Son du partage (#339, reprise de `son_wasapi.rs` et `screen_native.rs` de l'app Tauri) : trait `CaptureSon` de la crate plateforme
  (`capture/son.rs`, `capture/son_windows.rs`) ; `demarrer(CibleSon, recepteur)` rend un `FluxSon` qui arrête la capture quand il est lâché.
  Sous Windows, capture WASAPI « process loopback » (crate `wasapi`, Windows 10 2004 ou plus) en 48 kHz stéréo flottant sur un fil à part :
  une fenêtre donne le son de son processus et de ses enfants (`CibleSon::Application`, `SourceCapture::processus`), un écran tout le son du système
  sauf celui d'Estompe (`CibleSon::SystemeSauf`, processus d'Estompe exclu avec ses enfants : pas d'écho des voix du salon). Les échantillons sont ramenés au
  format de LiveKit par `ConversionEntree` (mono, 48 kHz, trames de 10 ms) et publiés sur une piste `ScreenshareAudio` (sans DTX), à côté de la vidéo.
  Option « Partager le son de cette fenêtre » ou « Partager le son de l'ordinateur (sans la voix des participants) » dans l'aperçu et dans « Qualité du partage »
  (préférence `partage.son`, activée par défaut comme `screenAudio`), masquée là où le trait est indisponible (Linux, macOS : lot 11) ou pour une fenêtre
  dont le processus est inconnu. Changer de source relance la capture du son sur la nouvelle cible ; l'option se change pendant le partage (piste ajoutée ou retirée).
  Si la capture du son échoue, le partage continue sans le son et la barre l'indique (« Partage sans le son : capture du son impossible. »).
  Non vérifié ici (machine Linux) : capture réelle sous Windows, absence d'écho, synchronisation du son et de l'image (moins de 100 ms visés).
- Incrustation de la caméra (#340, `camoverlay.ts` et `applyOverlay` du front ; `vocal/incrustation.rs` de la crate client) : ma caméra est composée en médaillon
  sur chaque image du partage avant l'encodage, les spectateurs la voient sans traitement de leur côté.
  - Réglages (`ReglagesIncrustation`, préférence `incrustation`, mêmes valeurs par défaut que `camOverlay`) : section « Incrustation de la caméra » de l'aperçu
    du sélecteur et de « Qualité du partage » ; interrupteur, coins, taille (10 à 50 % de la largeur), marge (0 à 120 px), rectangle arrondi ou cercle, bordure,
    ombre, miroir. Position en millièmes, taille et marge comme le front (`zone_incrustation` reprend `overlayRect`, `position_depuis_pointeur` le glisser de l'aperçu).
    Un changement s'applique tout de suite au partage en cours (`SessionVocale::regler_incrustation`) ; la préférence est écrite à la fin d'un glisser.
  - Aperçu : l'image de la source choisie est composée comme le partage (`ApercuIncruste`), recomposée à chaque réglage même si la source ne change pas ;
    glisser dans l'aperçu déplace le médaillon. Caméra coupée, une silhouette tient sa place. Pendant le partage, « Qualité du partage » montre ma tuile de partage.
  - Composition (`Compositeur`) : recadrage centré de la caméra (le cercle prend le carré central), coins arrondis (12 % du petit côté) ou cercle, bordure blanche,
    ombre portée et miroir comme le front ; le gabarit (formes, ombre, échantillonnage) n'est recalculé qu'au changement de réglage ou de taille, la vignette à chaque
    image de la caméra, puis elle est fondue sur l'image du partage. Coût mesuré (`cout_de_l_incrustation_a_1080p`, crate client en opt-level 3 sur la machine de dev) :
    1,6 ms par image en 1080p avec une nouvelle image de la caméra à chaque fois, soit 5 % d'un cœur à 30 images par seconde.
  - Active seulement avec la caméra et le partage en cours (`EtatVocal::incrustation`) : la session publie alors l'attribut `overlay`, comme le front. Une caméra
    incrustée (la mienne, ou celle d'un participant avec `overlay`) n'a pas de tuile et n'est pas reçue (`EtatVocal::camera_incrustee`).
  - Écran fixe : une capture qui n'envoie une image qu'au changement figerait la caméra ; la dernière image est gardée et recomposée toutes les 33 ms quand rien
    n'arrive depuis 45 ms (`Relance`, fil `estompe-incrustation`), comme le relais du front.
  - Caméra coupée pendant le partage : le médaillon disparaît (la dernière image est renvoyée sans lui) et le partage continue ; `overlay` revient à vide.
  - Écarts : pas de « Détourage libre » (il dépend de « Fond supprimé », lot 11) ni de réduction automatique par paliers ; sur une source qui ne change pas (Windows),
    la caméra de l'aperçu du sélecteur ne bouge qu'aux réglages.
- Écarts : le plafond de débit de l'encodeur est celui du début du partage (le SDK Rust 0.12 n'expose pas `setParameters` : changer la qualité règle la capture,
  pas le débit maximal) ; pas d'« Autre méthode » (sélecteur du système) ni de portail Wayland (lot 11) ; « Afficher le jeu en cours aux autres » (`SettingsShare` de l'app Windows)
  est dans les réglages « Jeux et activité » (voir [Jeu au premier plan](#jeu-au-premier-plan)) ; pas d'avis « Le système limite la capture à … images/s ».

### Appuyer pour parler

`PousserPourParler` (crate client) ferme la porte du micro (`SessionVocale::definir_porte`) pendant un appel quand le réglage est actif, l'ouvre à l'appui de la touche
et la referme au relâchement plus le délai choisi (0 à 2 s, préférence `ptt`, défaut : `KeyV` sans délai comme le front). La détection de voix est suspendue pendant ce temps
(`gateOpen` du front). La porte survit à un changement de salon (`EtatVocal::reinitialiser` la garde).

- Fenêtre active : la touche est lue par le `FocusScope` de la coquille (sauf dans un champ de saisie, comme le front) ; le raccourci global est alors ignoré, pour ne pas doubler la touche.
  Une perte de focus relâche la touche.
- Fenêtre inactive : raccourci global de la crate plateforme, enregistré seulement pendant un appel avec le réglage actif. Touches seules (lettres, chiffres, F1 à F24, Espace, Tab,
  modificateurs gauche et droite, flèches, navigation), comme l'app Tauri : pas de bouton de souris.
  - Windows : crochet clavier bas niveau (`WH_KEYBOARD_LL`) sur un fil dédié ; il rend toujours la main (`CallNextHookEx`) : la touche continue de parvenir au jeu.
    La touche est celle du clavier (code virtuel), comme le caractère tapé dans le réglage.
  - Linux X11 : évènements bruts XInput 2 (`x11rb`, Rust pur) sur la fenêtre racine, sans saisie exclusive ; la lettre tapée est retrouvée par la table du clavier.
  - Wayland (ou sans serveur X) : indisponible ; le réglage l'indique et la touche marche fenêtre active.
- Choix de la touche : « Touche : V » puis la touche voulue (`touche-choisie`), traduite en code par `src/vocal/touches.rs`.

### Ce que le SDK Rust offre par rapport à `livekit-client`

| Fonction | `livekit-client` (web) | SDK Rust 0.12 |
|---|---|---|
| Capture et lecture | navigateur | à la charge de l'app (`NativeAudioSource`, `NativeAudioStream`) ; ou `PlatformAudio` (ADM de WebRTC : WASAPI, PulseAudio/ALSA), non retenu : pas de périphérique factice, pas de volume par participant, pas de point d'insertion pour `nnnoiseless` |
| Écho, gain, bruit | contraintes `getUserMedia` | `AudioProcessingModule` exposé (`webrtc::native::apm` : annulation d'écho, gain automatique, passe-haut, suppression de bruit) sur des trames de 10 ms `i16`, micro (`process_stream`) et son joué (`process_reverse_stream`) ; utilisé par la chaîne du micro |
| Volume par piste | `setVolume` (jusqu'à 100 %, au-delà avec `webAudioMix`) | absent : mixage de l'app (`Mixeur`, 0 à 300 %) |
| Qualité de connexion | `ConnectionQualityChanged`, `connectionQuality` | idem (`ConnectionQualityChanged`, `connection_quality()`) |
| Qui parle | `ActiveSpeakersChanged` | idem |
| Reconnexion | reprise puis reconnexion complète | idem (10 essais, 60 s au plus), puis `Disconnected` |
| Simulcast, dynacast, flux adaptatif | oui | oui (`TrackPublishOptions::simulcast`, `RoomOptions::dynacast`, `adaptive_stream`), utiles au lot 5 |
| Attributs du participant | `setAttributes` | `set_attributes` |

### CRT statique sous Windows

libwebrtc est précompilé avec la bibliothèque standard de MSVC en `/MT` (`RuntimeLibrary=MT_StaticRelease`), les binaires précompilés de Skia en `/MD` :
l'éditeur de liens refuse de les réunir. `apps/estompe/natif/.cargo/config.toml` passe tout le binaire en CRT statique (`+crt-static`, comme l'app Tauri) ;
Skia n'existe pas en binaire précompilé pour ce CRT et `skia-bindings` le compile alors depuis ses sources (LLVM, Python 3 et `ninja`, long au premier build, puis en cache).
Le STL de MSVC récent (Visual Studio 18) refuse un clang plus ancien que 22 : `LLVM_HOME` et `LIBCLANG_PATH` désignent un LLVM 22 ou plus récent (sinon `skia-bindings`
prend `C:\Program Files\LLVM`), et `git config --global core.longpaths true` évite « Filename too long » au clonage des dépendances de Skia.

### Tests

- `audio::micro::tests` : cas de `micprocessor.test.ts` (`niveau_db`), porte (`gateOpen`), maintien de la détection de voix, gain borné, silence porte fermée,
  bruit hors parole et débuts de mots sur l'enregistrement versionné ; `audio::essai::tests` : niveau et retour du test du micro.
- `vocal::mixage::tests` : cas de `volume.test.ts` (`gainOf`), volume général, dix pistes à pleine échelle sans écrêtage dur, aucune latence ajoutée, référence d'écho.
- Mesure du coût (ignorée par défaut) : `cargo test --release -p estompe-client --features livekit --lib -- --ignored --nocapture cout`.
- `audio::tests` (périphériques factices) : capture à 44,1 kHz stéréo puis lecture à 48 kHz pendant 10 minutes simulées, sans perte ni latence croissante ;
  micro et sortie débranchés : bascule sur le périphérique par défaut, retour du choisi.
- `vocal::tests` (faux serveur, temps simulé) : deux participants se voient et s'entendent, quitter, refus du jeton, écoute seule, micro et casque publiés,
  coupure imposée, coupure du réseau de 20 s (retour sans action, micro conservé), reconnexion du SDK, doublon d'identité, déplacement par un modérateur, passage d'un salon à l'autre, fermeture, changement de micro à chaud,
  volume d'un participant retrouvé après avoir quitté puis rejoint, détection de voix (silence sous le seuil, niveau mesuré micro coupé).
- `vocal::livekit::tests` (feature `livekit`) : l'annulation d'écho de l'APM retire le son joué repris par le micro.
- `crates/client/tests/livekit.rs` (feature `livekit`) : contre le serveur LiveKit de `deploy/docker-compose.dev.yml` (`ESTOMPE_LIVEKIT_URL`, clé `devkey` du mode `--dev`),
  deux clients se voient, s'entendent, et la fermeture de l'un le retire de la salle chez l'autre en moins de 5 s. Sans la variable, il est ignoré et le dit.
  `just estompe-natif-vocal` le lance ; la CI Linux démarre ce serveur.
- `vocal::video::tests` : conversions (taille, couleurs, formats de caméra, miroir), mire, récepteur (dernière image, compteur, fermeture), un récepteur par flux ;
  `vocal::tests::seules_les_videos_affichees_sont_recues` (faux serveur) : abonnement à la demande avec la taille de la tuile, images reçues, désabonnement sans plus
  aucune image reçue, piste retirée, départ de son auteur ; `natif/src/video/` : conversion à la taille et en miroir, une seule image en route vers l'interface.
- `vocal::camera::tests` : qualités du front, MJPEG décodé, image trop courte ignorée, aperçu (images reçues, caméra occupée, introuvable, débranchée), caméra libérée à la fermeture ;
  `vocal::tests` : caméra publiée au format de la qualité, images reçues par l'autre participant, changement de caméra sans retrait, changement de qualité, retrait ;
  caméra occupée, débranchée pendant l'appel ou absente (retour à « désactivée » avec la raison) ; rejoindre caméra activée ; `crates/plateforme/src/camera.rs` : messages du système en cause.
- `crates/client/tests/livekit.rs::la_camera_est_publiee_changee_puis_retiree` (feature `livekit`, `ESTOMPE_LIVEKIT_URL`) : caméra factice publiée, images reçues par l'autre client, changement de caméra, retrait vu par l'autre client.
- Partage d'écran : `vocal::partage::tests` (cas de `screen.test.ts` : plafonds du serveur, paliers, fluidité et netteté, débit, préréglages, conversion RGBA vers I420) ;
  `vocal::tests` (faux serveur, sources factices `EcransFactices` de la crate plateforme) : publication au format de la qualité, images reçues par l'autre participant et ma tuile,
  changement de source sans retrait, changement de qualité, arrêt, source fermée ou en échec (message), plafond du serveur et taille de la source, réduction automatique par palier,
  incrustation (`la_camera_est_incrustee_dans_le_partage_puis_disparait_quand_elle_est_coupee` : médaillon composé, `overlay` publié, caméra plus reçue par l'autre,
  image recomposée sur écran fixe, médaillon retiré sans arrêter le partage), son de la fenêtre publié et entendu (cible du processus), son du système sans Estompe pour un écran, son retiré puis rendu, partage sans le son
  quand le trait est indisponible ou échoue ; `capture::son::tests` (conversions de `son_wasapi.rs`, cible du son, son indisponible hors Windows, flux factice) ;
  `natif/src/vocal/pont/partage.rs` : option du son masquée sans capture du son, pointeur de l'aperçu ramené dans la source, projection de l'incrustation ;
  `vocal::incrustation::tests` (calculs de `overlayRect` et du glisser, pixels attendus dans et hors du médaillon sur des images de synthèse, cercle, miroir, bordure,
  ombre, caméra coupée, aperçu recomposé) ; `crates/client/tests/livekit.rs::le_partage_est_publie_change_de_source_et_de_qualite_puis_retire` (feature `livekit`, `ESTOMPE_LIVEKIT_URL`) ;
  `natif/tests/vocal.rs` : sélecteur (écrans, fenêtres sans celles d'Estompe, miniatures), aperçu, qualité grisée au-delà de la source, publication, bandeau et rappel de la barre,
  ma tuile en grand, changement de source et de qualité pendant le partage, source perdue, Alt+Maj+S, « Arrêter », incrustation réglée dans le sélecteur
  (interrupteur, coin, médaillon visible dans l'aperçu, préférence retenue).
- `vocal::ptt::tests` (trait factice, temps simulé) : porte ouverte à l'appui, fermée au relâchement plus le délai, nouvel appui pendant le délai, raccourci ignoré fenêtre active,
  perte du focus, raccourci indisponible (message, touche fenêtre active) ; `audio::tonalite::tests` ; `store::reducteur::tests::transmet_le_deplacement_par_un_moderateur`.
- `crates/plateforme/src/raccourcis/` : table des touches, appui et relâchement sans répétition ; sous X11 (`DISPLAY`, ignoré sinon) une touche simulée par XTest est signalée
  et parvient toujours à la fenêtre active.
- `natif/src/vocal/projection.rs` : grille, soi-même affiché dès la connexion, halo jamais micro coupé, coupures du serveur, vue, barre, menu selon le rang (coupures,
  destinations du déplacement, déconnexion), clé de l'avatar, jauge ; `coquille/projection.rs` : durée du salon depuis le plus ancien participant ; `audio::tonalite::tests` : sons du front, volume.
- `natif/tests/vocal.rs` (backend de test Slint, faux serveur vocal, périphériques factices) : rejoindre depuis le panneau, tuiles et participants du panneau, indicateur de parole
  (mesuré : voir ci-dessous), photo de profil dans la tuile et le panneau, durée du salon, mode plein cadre (bouton, Échap, bouton de sortie), couper le micro, Alt+Maj+M et Alt+Maj+D,
  menu d'un participant (volume, muet, « Déplacer vers » et « Déconnecter du vocal » envoyés au faux serveur HTTP), coupure d'un modérateur, appuyer pour parler sans raccourci global,
  caméra d'un participant (piste factice 1080p) affichée dans sa tuile à la taille de la tuile, fenêtre réduite : désabonnement et plus aucune image reçue,
  fenêtre rouverte : retour des images, caméra retirée : retour à la photo, « Activer la caméra » (aperçu affiché avant publication, liste des caméras, choix de la caméra et
  de la qualité retenus, publication au format choisi, ma caméra dans ma tuile, Alt+Maj+V qui la coupe et libère la caméra, caméra occupée signalée dans l'aperçu),
  glisser un participant sur un autre salon (salons proposés pendant le glisser, déplacement envoyé, refus du serveur affiché puis effacé),
  choix du son et volume des sons retenus, son personnalisé (fichier trop gros refusé, fichier gardé, aperçu joué, retour au son prédéfini puis au fichier),
  déplacement par un modérateur, raccrocher, rejoindre depuis le bouton, refus « salon complet ».
- `audio::son_perso::tests` : WAV stéréo à 44,1 kHz, flottant, 8 et 24 bits, Ogg Opus sans ffmpeg, MP3 par ffmpeg (ignoré sans ffmpeg, exigé par `ESTOMPE_FFMPEG_EXIGE`),
  refus au-delà de 150 Ko ou d'un fichier illisible, fichier gardé et remplacé, lecture au volume des sons ; `vocal::projection::tests` : qui peut glisser un participant.
- Captures : `coquille-vocal-clair` (écran pour rejoindre), `vocal-1`, `vocal-4` (photos de profil, durée du salon), `vocal-video` (tuile avec l'image de la mire), `vocal-camera-apercu` (fenêtre « Activer la caméra » avec l'aperçu), `vocal-camera-erreur` (caméra occupée), `vocal-camera-active` (ma caméra en miroir dans ma tuile), `vocal-partage-sources` (sélecteur), `vocal-partage-apercu`, `vocal-partage-aucune-source`, `vocal-partage-en-cours` (bandeau, ma tuile, barre), `vocal-12`, `vocal-reconnexion`, `vocal-menu`,
  `vocal-moderation` (déplacer, déconnecter), `vocal-plein-cadre`, `vocal-reglages`, `vocal-sons`, `vocal-sons-perso` (« Mon fichier » choisi, refus d'un fichier),
  `vocal-glisser` (salons proposés pendant le glisser), `vocal-glisser-refus`.

## Intégration au bureau

`src/bureau/` (`Bureau`), sur les traits de la crate plateforme ; les rappels qui viennent d'un autre fil (instance, zone) reviennent au fil de l'interface par `slint::invoke_from_event_loop`.

- Instance unique (`instance.rs` de la crate plateforme, `interprocess`) : au lancement, avant toute fenêtre, l'app se connecte au socket de l'instance
  déjà lancée. Si elle répond, elle lui envoie ses arguments (`MessageInstance` : arguments, jeton d'activation `XDG_ACTIVATION_TOKEN` ou
  `DESKTOP_STARTUP_ID` ; JSON d'une ligne, 64 Kio au plus, 2 s au plus) et sort avec le code 0. Sinon elle devient la première instance et écoute
  (fil `estompe-instance`).
  - Linux : socket de fichier `$XDG_RUNTIME_DIR/fr.plbls.estompe.natif/instance.sock` (dossier en 0700 ; sans `XDG_RUNTIME_DIR`, `<données>/instance.sock`),
    jamais dans l'espace abstrait (ouvert à tous les comptes). Un socket resté d'un plantage (connexion refusée) est retiré puis recréé.
  - Windows : tube nommé `\\.\pipe\fr.plbls.estompe.natif-<empreinte du dossier de données>` (le chemin porte le compte) ; la seconde instance
    appelle `AllowSetForegroundWindow` avant d'écrire, pour que la première puisse passer au premier plan.
  - `ESTOMPE_NATIF_DOSSIER` donne une autre instance (`<dossier>/instance.sock` sous Linux) : deux dossiers, deux instances.
  - La première instance ramène la fenêtre (`show()`, puis par winit : visible, non réduite, `focus_window()`), sauf si les arguments portent
    `--arriere-plan` ; `--invitation <code>` mène à « Rejoindre un serveur » avec ce code (`Evenement::OuvrirInvitation` puis `Evenement::Rejoindre`).
  - L'app Tauri utilise le greffon single-instance sous l'identifiant `fr.plbls.estompe` : les deux apps ne se ramènent jamais l'une l'autre.
- Zone de notification (`zone.rs` de la crate plateforme, `tray-icon` ; sous Linux, son moteur `ksni`, StatusNotifierItem par D-Bus, sans GTK) :
  icône de l'app (64 px), info-bulle « Estompe natif (bêta) », menu « Ouvrir Estompe natif » et « Quitter » ; un clic ou « Ouvrir » ramène la fenêtre.
  L'icône est créée sur le fil de l'interface (boucle de messages de winit sous Windows).
- Fermeture : la demande de fermeture de la fenêtre la masque (la géométrie est retenue avant) et garde la session et la connexion temps réel,
  si la zone est présente : toujours sous Windows, sous Linux si le nom `org.kde.StatusNotifierWatcher` a un propriétaire sur le bus de session
  (vérifié à chaque demande : une zone apparue après le lancement compte). Sans zone, fermer quitte.
- Quitter (`Fermeture::quitter`, seule façon de quitter proprement ; « Quitter » de la zone, fermeture sans zone, fin de la boucle) : retient la
  géométrie, arrête la coquille et attend la fin de sa session (3 s au plus : flux temps réel fermé, brouillons écrits), étape « vocal » (vide jusqu'au
  lot 4), retire l'icône, puis quitte la boucle d'évènements (`run_event_loop_until_quit` : masquer la fenêtre ne quitte plus l'app).
- Démarrage avec la session (`demarrage.rs`, `DemarrageSysteme`) : sous Windows, valeur `EstompeNatif` de
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, `"<exe>" --arriere-plan` ; l'interrupteur « Lancer Estompe natif à l'ouverture de la
  session » (section « Bureau » de « À propos ») lit et écrit le registre, désactivé par défaut. Sous Linux, indisponible et masqué jusqu'au paquet (lot 11).
- Section « Bureau » de « À propos » : « Notifications du bureau » (préférence `notifications`) et, sous Windows, le démarrage avec la session.
- Non vérifié sur machine réelle : icône et menu sous Windows, retour depuis un autre bureau virtuel, rendu dans une zone StatusNotifierItem (KDE,
  GNOME avec l'extension AppIndicator) ; sous Wayland, le jeton d'activation n'est pas transmis à winit (seul `focus_window()` est tenté).

## Notifications

Règles dans la crate client (`notifications.rs`, sans code d'OS), reprises du front (`showNotification` de `blocks/notifications/notify.ts` et de `store.svelte.ts`) :

- L'app ne notifie que sur l'évènement temps réel `notification` (`Changement::Notification` du store), jamais sur `message.created` : niveaux,
  sourdines, « ne pas déranger », mentions et types de notification sont décidés par le serveur (`blocks/notifications`) ; réglages : voir [Paramètres](#paramètres).
- `decider` : rien pour l'activité vocale de mon propre salon ; rien si la conversation de la notification est affichée **et** la fenêtre active
  (focus de winit) ; sinon une notification du système si la préférence « Notifications du bureau » est active.
- Regroupement : l'étiquette (`tag` du serveur) remplace une notification de même étiquette (toast de même `Tag`, `replaces_id` sous Linux).
- Son : celui du système avec la notification (audio par défaut du toast, indice `sound-name=message-new-instant` sous Linux), muet si le serveur la dit
  silencieuse. Écart assumé : le front joue un bip même quand les notifications du bureau sont coupées ; l'app native n'en joue pas (la sortie du son, `SortieSon`, ne sert encore qu'aux messages vocaux).
- Actions (`effet_de_l_action`) : un clic ramène la fenêtre et ouvre la conversation au message (adresse du serveur sans « / » initial, `#/s/…/<message>`
  ou `#/dm/…`, puis `sauter_a`) ; « Marquer comme lu » lit la conversation (`Store::lire_conversation` : `POST …/read` et lecture des mentions) sans
  naviguer ; « Répondre » avec un texte l'envoie par la file d'envoi (`Store::envoyer`, message simple de la conversation), sans texte ouvre la conversation.
  Les actions ne sont proposées que si la notification porte un message et une conversation.

| | Windows | Linux |
|---|---|---|
| Affichage | toast WinRT (`toast_windows.rs`, XML de `toast.rs`), AUMID `fr.plbls.estompe.natif` enregistré dans `HKCU\Software\Classes\AppUserModelId\fr.plbls.estompe.natif` (`DisplayName` « Estompe natif (bêta) », `IconUri` = `estompe.ico` à côté de l'exe) au lancement | `notify-rust` (D-Bus, `org.freedesktop.Notifications`, `notifications_linux.rs`) |
| Actions | clic, champ « Répondre » et bouton « Marquer comme lu » | clic (`default`) et « Marquer comme lu » si le serveur annonce `actions` ; pas de réponse en ligne (la spécification n'en a pas) ; 8 notifications attendues au plus, la plus ancienne est fermée |
| Pastille de mentions | barre des tâches (`ITaskbarList3::SetOverlayIcon`, `pastille_windows.rs`) et icône de la zone | icône de la zone ; barre des tâches au lot 11 (pas de standard sans `.desktop`) |

- Natif : `src/bureau/notifications.rs` (`Notifieur`) observe la session de la coquille (`Coquille::observer`, trait `ObservateurSession`) : décision,
  affichage, table des 64 dernières notifications pour retrouver une action par son étiquette. La pastille suit `total_mentions()` à chaque lot de
  changements, et n'est redessinée que si le nombre change : disque `#d9342b` et chiffres blancs d'une police bitmap embarquée, « 9+ » au-delà de 9
  (`pastille.rs` de la crate client), aussi posé dans un coin de l'icône de la zone ; effacée à la fin de la session.
- Journal : seulement `notification affichée (genre, conversation)` ; ni titre, ni corps, ni texte de réponse.
- Sans activateur COM, un clic sur un toast resté dans le centre de notifications peut ne rien rappeler quand l'app tourne : à vérifier sur machine réelle,
  comme le toast avec ses boutons (app installée), la réponse et la pastille de la barre des tâches ; sous Linux, l'affichage et le clic sous un vrai bureau.

## Mises à jour

Logique dans la crate client (`mise_a_jour/`, sans code d'OS), lancement de l'installeur par la crate plateforme (`Installation`), service
démarré par `src/mise_a_jour.rs` sur le runtime de l'entrée.

- Manifeste : `https://github.com/Plblsfr/plbls.fr/releases/download/estompe-natif-beta/natif.json` (format dans [Livraison](#livraison)),
  lu par `Manifeste` (champs de tête de `version::Publication`, paquet par cible : `windows-x86_64`). Une mise à jour n'est proposée que si la
  version publiée est plus récente que `VERSION` et que le manifeste a un paquet pour la cible ; `mandatory`, `force` et `minVersion` l'imposent
  (`version::disponibilite`). `ESTOMPE_NATIF_MANIFESTE` remplace l'adresse pour un essai, seulement si elle passe la politique ci-dessous.
- Adresses (`politique.rs`, `PolitiqueAdresses::production`) : HTTPS seulement, hôte exactement `github.com`, `objects.githubusercontent.com` ou
  `release-assets.githubusercontent.com`, sans identifiants ni port explicite ; la même règle est réappliquée à chaque redirection (5 au plus) :
  une redirection hors de la liste arrête le téléchargement.
- Téléchargement (`telechargement.rs`, client `reqwest` dédié) dans `<cache>/mises-a-jour/Estompe-natif-beta-<version>-setup.exe.partiel` :
  reprise par `Range: bytes=<n>-` (206 : suite ajoutée ; 200 : repart de zéro ; 416 : partiel effacé, nouvel essai), trois essais quand le réseau coupe,
  taille plafonnée par `taille` du manifeste (200 Mo sans elle). Une coupure garde le partiel pour reprendre ; une erreur de disque (plein, droits)
  le supprime avec un message lisible ; un fichier plus court que `taille` est supprimé.
- Signature (`signature.rs`, `minisign-verify`, clé publique `mise_a_jour/cle.pub` embarquée) : `signature` du manifeste (fichier `.minisig` en
  base64), signature préhachée seulement, et commentaire de confiance exactement `estompe-natif <version>` : un installeur signé d'une autre
  version ne peut pas être resservi sous un autre numéro. Fichier vérifié : renommé sans `.partiel` ; refusé : supprimé, rien n'est exécuté et
  l'erreur le dit. L'installeur est revérifié juste avant d'être lancé.
- Installation (Windows, `installation.rs` de la crate plateforme) : `Estompe-natif-beta-<version>-setup.exe /S /RELANCER`, processus détaché ;
  si le lancement réussit, l'app quitte proprement (`Fermeture::quitter`) ; l'installeur attend sa sortie, installe et la relance, la session
  reprend par le refresh token du coffre. Un installeur qui ne démarre pas laisse l'app en place avec l'erreur.
- Machine (`machine.rs`, sans effet de bord, comme `entree.rs`) et service (`service.rs`, état publié par `tokio::sync::watch`) : vérification
  15 s après le lancement, puis toutes les 30 minutes, et au retour de la fenêtre au premier plan (au plus une fois par 5 minutes) ; une
  vérification ou une installation à la fois ; l'erreur d'une vérification n'est montrée que si elle a été demandée, celle d'une installation toujours.
  Une mise à jour imposée bloque l'app (écran « Mise à jour requise » de l'entrée), ou après 120 s si l'utilisateur est occupé (texte ou pièces jointes
  dans la zone de saisie, message vocal en cours, ou en appel). En appel, l'installation attend la fin de l'appel ou une confirmation (aucun vocal avant le lot 4 : jamais en appel aujourd'hui).
- Au démarrage, les installeurs de `mises-a-jour/` d'une version inférieure ou égale à la version installée sont supprimés.
- Journal : version disponible, téléchargement vérifié ou refusé, lancement ; jamais de jeton.
- Linux : `Installation` indisponible, aucune vérification (le paquet Linux arrive au lot 11).
- Interface (`ui/mise-a-jour/`, `src/mise_a_jour.rs`) : l'état du service est recopié dans le global `DonneesMiseAJour` (`afficher`, par
  `upgrade_in_event_loop`), ses rappels renvoient au service (`brancher`).
  - Bandeau (`bandeau.slint`, en bas du panneau de la coquille, comme `UpdateNotice.svelte`) : « Nouvelle version disponible : Estompe natif X.Y.Z »,
    notes de version sur deux lignes puis « Plus », « Mettre à jour » (devient « NN % » puis « Installation… », barre de progression) et « Plus tard »
    (la version est écartée jusqu'à la suivante) ; pendant le compte à rebours d'une mise à jour imposée, « Mise à jour obligatoire dans M:SS ».
  - Panneau (`panneau.slint`, section « Mises à jour » de « À propos », rangé dans les réglages au lot 8) : version installée, « Rechercher une mise à
    jour » (vérification manuelle : son erreur est montrée), « Tu as la dernière version », version disponible et ses notes, « Installer et
    redémarrer » avec la progression, « Installer maintenant » en appel, erreur lisible en `Theme.danger`. Sous Linux : lecture seule, « Les mises à
    jour automatiques arriveront avec le paquet Linux ».
  - Écran forcé : voir [Mise à jour forcée](#mise-à-jour-forcée).
- Non vérifié sur machine réelle : passage d'une version publiée à la suivante (deux tags), relance par l'installeur et session conservée.

## Jeu au premier plan

Savoir quel jeu tourne, pour l'activité montrée aux autres, puis pour les overlays et l'instant replay (lot 7). Copié de `foreground.rs` et `jeux/` de l'app Tauri, puis divergé.

- Plateforme : trait `PremierPlan` (`apps/estompe/crates/plateforme/src/premier_plan/`). `application()` décrit la fenêtre au premier plan (`ApplicationPremierPlan` :
  exécutable, chemin complet, titre, plein écran sans barre de titre, écran qui la contient, poignée de la fenêtre, processus) ; `fenetres()` les fenêtres
  visibles, titrées, hors fenêtres d'outils, une par processus. Sous Windows : `GetForegroundWindow`, `QueryFullProcessImageNameW`, `MonitorFromWindow`, `EnumWindows`.
  Linux : indisponible (`PremierPlanSysteme::disponible()` faux), les réglages et l'entrée du menu sont masqués. `PremierPlanFactice` sert aux tests.
- Reconnaissance (`apps/estompe/crates/client/src/jeux/`) : liste intégrée `connus.json` (nom de l'exécutable ou morceau du chemin), dossier qui suit une bibliothèque
  (Steam, Epic, GOG, Xbox), sinon une application plein écran sans bordure au premier plan ; jamais les outils de Windows, navigateurs, lecteurs, Estompe (`hors_jeu`).
  Un jeu ajouté à la main (chemin complet) est toujours un jeu ; un jeu exclu (nom de l'exécutable) ne l'est jamais, ni au premier plan ni parmi les fenêtres ouvertes,
  et n'est donc ni affiché ni publié. Sans jeu au premier plan, un jeu connu ou ajouté parmi les fenêtres ouvertes reste « en cours » (alt-tab).
- Sondage (`ServiceJeux`, `apps/estompe/natif/src/jeux/`) : toutes les 5 s sur le runtime de l'app, et tout de suite après un changement de réglage. Il publie `EtatJeux`
  dans un `watch` (`Application::jeux`, `ServiceJeux::abonner`) : jeu au premier plan (avec son écran et sa fenêtre, pour les overlays et le replay), activité,
  dernière application vue au premier plan hors Estompe. Les fenêtres ne sont énumérées que si aucun jeu n'est au premier plan. Coût de la reconnaissance mesuré
  (`cout_du_sondage`, crate client en release, machine de dev Linux, 30 fenêtres) : 33 µs par sondage, soit moins de 0,001 % d'un cœur ; les appels Windows
  (une fenêtre, un processus ouvert, et `EnumWindows` hors jeu) ne sont pas mesurés ici.
- Activité : le jeu détecté reste affiché une minute après sa disparition (`DELAI_DE_GRACE`, alt-tab ou redémarrage), comme `activity.svelte.ts` ; exclure le jeu en
  cours l'efface tout de suite. Il est toujours visible sous mon pseudo (`activite_locale` du store) ; il n'est envoyé aux autres (`PUT /me/activity`, 64 caractères au plus)
  que si « Afficher le jeu en cours aux autres » est actif (désactivé par défaut, préférence `jeux.partager`). L'envoi se fait à chaque changement et à chaque reconnexion
  (le serveur oublie l'activité à la déconnexion) ; un échec est retenté au changement suivant (`Publication`, `Annonce` dans le cycle de la coquille).
- Réglages « Jeux et activité » (`ui/jeux/reglages.slint`, entrée du menu de l'utilisateur) : interrupteur du partage, jeu en cours, dernière application vue au premier plan
  avec « Ajouter comme jeu » et « Exclure » (passer sur le jeu puis revenir dans Estompe), jeux ajoutés (« Ajouter un jeu… » par le sélecteur de fichiers), jeux exclus
  (nom saisi ou « Parcourir… »). Préférence `jeux` (`ReglagesJeux` : listes sans doublon à la casse près, 200 entrées et 260 caractères au plus).
- Écarts : les jeux ajoutés et exclus étaient propres au replay dans l'app Tauri (`SettingsReplay`) ; ici ils servent à toute la détection (activité, overlays, replay).
  Pas de « Écoute » (musique) dans l'activité avant le lot 10.
- Non vérifié ici (machine Linux) : détection d'un vrai jeu sous Windows (connu et ajouté à la main), activité vue par un autre client, coût CPU réel du sondage sous Windows.

## Overlay

Fenêtres transparentes par-dessus le jeu, dessinées par Slint dans le processus de l'app. Porté de `overlay_natif/` de l'app Tauri (prototype de l'issue #56,
voir [overlay-natif.md](overlay-natif.md)) et des composants `Overlay.svelte`, `overlay.ts` et `SettingsOverlay.svelte`. L'overlay des participants du vocal (#353)
est la première scène ; celui des messages (#354) et l'indicateur du replay (#356) s'ajoutent comme d'autres scènes du même moteur.

- Plateforme : trait `Overlay` (`apps/estompe/crates/plateforme/src/overlay/`), fabrique de `Calque`s. Un calque affiche une `ImagePremultipliee` (RGBA prémultiplié)
  à une position en pixels physiques, avec une opacité globale, ou se masque. Sous Windows (`OverlaySysteme`, repris du prototype) : fenêtre
  `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` mise à jour par `UpdateLayeredWindow` (transparence par pixel, opacité
  par `SourceConstantAlpha`), jamais de focus, aucun clic capturé, absente de la barre des tâches. `pomper()` traite les messages des fenêtres du fil ;
  `ecran_principal()` et `echelle()` (DPI effectif de l'écran, `GetDpiForMonitor`) servent au placement. Linux : indisponible, rien n'est créé et la section des réglages
  est masquée. `OverlayFactice` retient chaque calque (image, position, opacité) pour les tests.
- Moteur (`MoteurOverlay`, `apps/estompe/natif/src/overlay/mod.rs`) : un fil `estompe-overlay`, démarré au premier affichage, possède sa propre plateforme Slint
  (la plateforme de Slint est propre à chaque fil) : une `MinimalSoftwareWindow` par composant, rendu logiciel, ni winit ni GPU. Une scène
  (`MoteurOverlay::scene(Composant::new, appliquer)`) donne une `SceneOverlay<E>` : `afficher(etat, placement, opacite)` n'envoie rien si l'affichage est identique au
  précédent ; sinon le fil applique l'état au composant, le rend à la taille demandée (facteur d'échelle de l'écran) et le passe à son calque. `masquer()` cache le calque.
  Aucun rendu sans changement : `rendus()` compte les images rendues, `demandes()` les affichages envoyés. Le fil pompe les messages après chaque
  commande, et toutes les 100 ms sans commande (10 ms pendant une saisie).
- Participants du vocal (`overlay/voix.rs`, `ui/overlay/voix.slint`) : `PontVocal::projeter` calcule les participants du salon où je suis connecté
  (`projection::participants`, mêmes états que les tuiles : je parle, micro et casque coupés) et les passe à `ServiceOverlayVoix` avec leur photo de profil (chargée
  par `Profils` au besoin, recadrée en disque sur le fil de l'overlay car le moteur logiciel ne découpe pas une image par un `border-radius`). Le service suit aussi
  `EtatJeux` (`ServiceJeux::abonner`) et les réglages, et calcule l'affichage (`voix::calculer`) : rien hors du vocal, hors jeu en mode « seulement en jeu », ou sans
  personne à montrer ; sinon l'écran du jeu, à défaut le dernier écran d'une application au premier plan, à défaut l'écran principal. Rendu comme `Overlay.svelte` :
  avatars de 28, 40 ou 56 px, contour et halo `#4ade80` du locuteur, initiales sur `#444`, pseudo dans une pastille `#00000099`, icône du micro ou du casque coupé.
- Calculs (`apps/estompe/crates/client/src/overlay.rs`, repris de `overlay.ts`) : participants retenus (tous ou ceux qui parlent), visibilité, taille de la fenêtre
  (largeur de l'avatar + 220, une ligne de l'avatar + 8 par participant), position dans l'un des six coins avec une marge de 16 px logiques, placement en pixels physiques.
- Réglages (section « Overlay des participants » de « Jeux et activité », `ui/overlay/reglages.slint`, `overlay/reglages.rs`) : activer, seulement quand un jeu est au
  premier plan, emplacement, taille, participants affichés, opacité (30 à 100 %). Préférence `overlay` (`ReglagesOverlay` : `quand` jeux, toujours ou jamais, `taille`,
  `coin`, `mode`, `opacite`), mêmes valeurs par défaut que le front (seulement en jeu, centre gauche, moyen, tous) et opacité pleine.
- Écarts : pas de raccourci global pour masquer ou réafficher l'overlay (`overlayKey` de l'app Tauri) ; opacité en plus. Comme dans l'app Tauri, une fenêtre « layered »
  n'apparaît pas au-dessus d'un jeu en plein écran exclusif : seuls le fenêtré et le plein écran sans bordure sont pris en charge.
- Tests : calculs de `overlay.test.ts` (`overlay::tests` de la crate client) ; moteur (`overlay::tests` : image transparente, halo, pas de rendu pour un état identique,
  un calque par scène, rien sans overlay) ; service (`overlay::voix::tests`, dont `sans_changement_d_etat_aucun_rendu_pendant_dix_secondes`, temps de tokio simulé,
  sondage du jeu toutes les 5 s et projections répétées) ; captures `tests/overlay.rs` comparées à `tests/references/overlay-voix-*.png` (1, 4 et 8 participants,
  locuteurs, micro et casque coupés, chaque taille ; composées sur un fond sombre) ; parcours dans `tests/vocal.rs` (trois participants à l'arrivée dans le salon, overlay
  masqué en raccrochant) et `tests/jeux.rs` (écran du jeu suivi, réglages appliqués, masqué hors jeu, « toujours », désactivé).
- Messages privés et mentions (#354, `overlay/messages.rs`, `ui/overlay/messages.slint`, calculs dans `apps/estompe/crates/client/src/overlay_messages.rs`, repris
  de `MsgOverlay.svelte`, `msg-overlay.svelte.ts` et `overlay.ts`) : le `Notifieur` du bureau passe chaque notification du serveur à `ServiceOverlayMessages`,
  indépendamment des notifications du bureau. Retenues selon le choix (privés, mentions, les deux, aucun) et rien en « Ne pas déranger » si la sourdine est cochée
  (`overlay_messages::voulu`). File comme l'app Tauri (`FileMessages`) : quatre bandeaux au plus, le même message remplacé, chacun retiré après sa durée (2 à 60 s,
  8 par défaut) ; pendant une saisie, les bandeaux échus restent et leur durée repart. Une minuterie tokio (`demarrer_minuterie`) attend la prochaine échéance ; l'heure
  vient d'une horloge injectable pour les tests. Rendu : bandeaux de 340 × 64 px `#000000bf`, photo de profil en disque ou initiales, titre en gras, corps sur une
  ligne (masqué si « masquer le contenu ») ; champ de réponse de 36 px sous les bandeaux, avec curseur et la fin du texte visible. Placement dans le coin choisi
  (haut droite par défaut) sur l'écran du jeu comme les participants ; affiché seulement en jeu par défaut.
- Réponse rapide : deux touches globales (`RaccourcisGlobaux`, une touche seule : F8 pour répondre au dernier bandeau, F7 pour ouvrir sa conversation), enregistrées
  seulement tant qu'un bandeau est affiché, jamais celle de l'appui pour parler. Répondre ouvre le champ puis demande le focus au calque (`SceneOverlay::saisir`,
  `Calque::saisir`) ; le fil de l'overlay pompe alors toutes les 10 ms. Sous Windows, le calque mémorise la fenêtre au premier plan (le jeu), retire
  `WS_EX_TRANSPARENT | WS_EX_NOACTIVATE` et prend le premier plan (`AttachThreadInput` puis `SetForegroundWindow`) ; sa procédure transmet `WM_CHAR` (texte, paires
  de substitution, Retour arrière, Entrée, Échap) et `WM_KILLFOCUS` (`EvenementSaisie`). Entrée envoie au dernier bandeau par la file d'envoi, marque la conversation
  lue et retire ses bandeaux ; Échap ou la perte du focus annulent. Dans tous les cas `rendre_le_focus` remet les styles et rend le premier plan au jeu, sans
  `ShowWindow` sur sa fenêtre (ni réduction ni restauration) ; si l'utilisateur est passé à une autre fenêtre (perte du focus), le premier plan n'est pas repris. « Ouvrir » ramène la fenêtre de l'app sur la conversation, comme le clic d'une notification. Linux :
  l'overlay est indisponible (section masquée) ; un overlay sans saisie (`Overlay::saisie_disponible`) ne prend que la touche « ouvrir » et le dit dans les réglages.
- Réglages des messages (section « Messages par-dessus le jeu » de « Jeux et activité ») : bandeaux affichés, seulement en jeu, sourdine en « Ne pas déranger »,
  masquer le contenu, emplacement, durée, touches « répondre » et « ouvrir » (capturées comme celle de l'appui pour parler, refusées si déjà prises). Préférence
  `overlay_messages` (`ReglagesMessages`), à côté de `overlay`.
- Écarts des messages : les touches sont une touche seule (comme l'appui pour parler) au lieu de Ctrl+Maj+R et Ctrl+Maj+E, et le crochet du clavier ne les retire
  pas au jeu ; emplacement et « seulement en jeu » propres aux messages (partagés avec les participants dans l'app Tauri) ; sourdine en « Ne pas déranger » vérifiée
  aussi par l'app.
- Tests des messages : `overlay_messages::tests` (genres, sourdine, file, durée, saisie, réponse, ouverture, taille, réglages) ; `overlay::messages::tests` (vue,
  expiration, touches seulement pendant l'affichage, focus pris puis rendu, Échap et perte du focus, « ouvrir », sans saisie, touche de l'appui pour parler,
  masqué hors jeu, minuterie en temps simulé) ; `plateforme::overlay::tests` (caractères de `WM_CHAR`, faux calque qui prend et rend le focus) ; captures
  `tests/references/overlay-messages-*.png` (message privé, mention, saisie vide et en cours) ; parcours dans `tests/bureau.rs` (mentions reçues en bandeaux, réponse
  par F8 envoyée au serveur et focus rendu, « ouvrir » par F7) et `tests/jeux.rs` (réglages des messages retenus et appliqués au bandeau).
- Non vérifié ici (machine Linux) : affichage réel sur un jeu fenêtré sans bordure sous Windows, absence de vol de focus, mémoire et CPU comparés à l'overlay de l'app Tauri
  (protocole de [natif-mesures.md](natif-mesures.md)) ; réponse depuis un vrai jeu (le message part, le jeu reprend le focus sans être réduit) et refus éventuel du
  premier plan par Windows.

## Instant replay

Garder en permanence les dernières secondes du jeu au premier plan et les enregistrer en clip MP4 (#355). Porté de `replay/` de l'app Tauri
(voir [instant-replay.md](instant-replay.md)), puis divergé : la capture et le son viennent des traits du lot 5, l'image est convertie par le processeur.

- Parties pures (`apps/estompe/crates/client/src/replay/`) : `tampon` (unités compressées en RAM, éviction par groupe d'images entier, plafonds en durée et en octets,
  instantané sans copie), `cadence` (une image par échéance, la dernière répétée, plus de 3 images de retard comptées perdues ; `encodeur_muet` ; alertes de perte ;
  `Alignement` comble les silences du son), `automate` (`inactif`, `arme`, `pause`, `indisponible`), `garde` (fichier `estompe-encodeur.txt` du dossier des journaux :
  une session d'encodeur ouverte puis un arrêt de l'app avant la première image désactive le replay jusqu'à la version suivante), `micro` (RNNoise et gain), `son`
  (stéréo ou mono à 48 kHz), `image` (RGBA vers NV12 BT.709 16-235 avec réduction), `reglages` (résolution, cadence, codec, débit, durée selon la RAM, `options` pour les
  réglages), `mp4` (`moov` en tête, pistes entrelacées par seconde), `ecriture` (fichier `.part` puis renommage, annexe JSON, disque plein, quota, file d'écriture sur un fil),
  `galerie` (nom des clips, dossier par défaut `Vidéos/Estompe`, annexes `.estompe/`, liste). Préférence `replay` (`ReglagesReplay` : actif, résolution, cadence, durée de
  15 à 300 s, qualité, micro, curseur, touche, dossier, quota), désactivée par défaut ; micro, réduction de bruit et gain viennent des réglages du vocal.
- Plateforme : trait `EncodageMateriel` (`apps/estompe/crates/plateforme/src/encodage/`) : `materiel(fenetre)` (carte, encodeurs H.264 et HEVC avec leurs limites,
  rafraîchissement de l'écran, RAM libre), `video(parametres, sortie, arret)` (images NV12 soumises avec leur horodatage, paquets Annex B rendus au récepteur, image clé
  par groupe, refus si saturé), `audio(parametres, sortie)` (PCM 16 bits vers AAC). Sous Windows (`EncodageSysteme`) : MFT matériel de Media Foundation (NVENC,
  QuickSync, AMD) sur la première carte « haute performance » qui en a un, débit constant, sans images B, entrée en mémoire vive ; AAC par le MFT de Windows.
  Linux : `Indisponible`, l'état du replay est `indisponible` (« instant replay disponible sous Windows uniquement ») et rien n'est capturé. `EncodageFactice` pour les tests.
  `disque_plein`, `masquer` (dossier caché) et `dossier_videos` sont aussi dans la crate plateforme.
- Service (`apps/estompe/natif/src/replay/`, `ServiceReplay`, `Application::replay`) : un fil `replay` suit `EtatJeux` (donc les jeux ajoutés et exclus de `p.jeux`) ;
  au jeu au premier plan il capture sa fenêtre (`CaptureEcran`, `fenetre:<n>`), attend la première image (3 s), choisit dimensions, cadence et codec, ouvre l'encodeur,
  le son du jeu (`CaptureSon`, processus du jeu) et le micro si coché (`SonPeripheriques`), puis un fil `replay-rythme` émet les images à cadence fixe. Hors du premier
  plan : pause, tout est fermé, le tampon est gardé ; fermeture du jeu (plus aucune fenêtre de son processus, vérifié toutes les 5 s) ou changement de jeu, de réglage
  vidéo ou audio : tampon vidé. Image du jeu qui change de taille : la session redémarre.
- API pour les réglages, l'indicateur, la galerie et l'éditeur : `configurer(ReglagesMoteur)`, `sauvegarder()` (durée des réglages), `sauvegarder_les_dernieres(n)`,
  `partage(actif, suspendre)`, `disponibilite()`, `options(&ReglagesReplay)`, `dossier()`, `etat()` (`watch` d'`EtatReplay` : état, jeu, format, encodeur, touche
  enregistrée) et `evenements()` (`broadcast` d'`EvenementReplay` : `Clip(ClipEnregistre)` ou `Erreur(ErreurReplay)` avec les codes de l'app Tauri : `tampon-vide`,
  `quota`, `disque-plein`, `dossier-inaccessible`, `capture-refusee`, `encodeur-muet`, `encodeur-arrete`, `encodeur-sature`, `capture-en-retard`, `son-indisponible`,
  `micro-indisponible`, `image-changee`).
- Raccourci : une touche seule (F10 par défaut) enregistrée par `RaccourcisGlobaux` tant que le replay est actif et disponible, jamais une touche déjà prise (appui pour
  parler, touches de l'overlay des messages).
- Touches globales (appui pour parler, overlay des messages, replay) : `Preferences::touches_des_autres(usage)` refuse dans chaque réglage une touche déjà prise par un
  autre usage actif (message « Cette touche sert déjà… ») ; à l'exécution, `touches_prioritaires(usage)` départage des préférences anciennes en conflit : l'appui pour
  parler passe avant les messages, qui passent avant le replay.
- Partage d'écran : `suivre_le_partage` suit `EtatVocal::partage` et appelle `partage(actif, false)`, comme `replay_partage` de l'app Tauri : le tampon continue pendant
  un partage, mais un échec de capture ou d'encodage pendant le partage met le replay en pause (« le partage d'écran est prioritaire ») au lieu de l'arrêter ; il repart à
  la fin du partage.
- Réglages (#356, `apps/estompe/natif/src/replay/reglages.rs`, `apps/estompe/natif/ui/replay/reglages.slint`) : fenêtre « Instant replay » ouverte depuis « Jeux et
  activité » (résumé de l'état et bouton « Régler l'instant replay… »). Activer (seulement si un encodeur est disponible), état du tampon et encodeur, touche (capturée
  comme celle de l'appui pour parler), résolution et images par seconde (choix hors de portée du matériel grisés, raison affichée), qualité, durée (15 s à 5 min par pas de
  15 s, plafonnée selon la mémoire libre), curseur, micro, rappel des jeux ajoutés et exclus (réglés dans « Jeux et activité »), dossier (choisir, par défaut, ouvrir avec
  `Fichiers::ouvrir_dossier`) et quota (1 Go au moins). Le matériel (`disponibilite`, `options`) est lu sur un fil à l'ouverture, puis relu quand un réglage de capture ou le
  jeu change. Toute valeur passe par `ReglagesReplay::borne` ; textes et bornes de l'interface dans `estompe_client::replay::affichage` (`duree_choisie`, `quota_choisi`).
- Indicateur sur le jeu (#356, `apps/estompe/natif/src/overlay/replay.rs`, `apps/estompe/natif/ui/overlay/replay.slint`) : une scène du moteur d'overlay, comme les
  participants et les messages. Pastille « Replay » tant que le tampon tourne, puis jusqu'à trois messages (vert : clip enregistré, jaune : pause ou reprise, rouge :
  erreur ou indisponible) ; chaque message reste la durée d'affichage des messages (2 à 60 s). Logique de `replay.ts` portée dans `estompe_client::replay::affichage`
  (`message_etat`, `message_clip`, `message_erreur`, `FileReplay`, `taille_fenetre`). Placé sur l'écran du jeu, dans le premier coin du haut puis du bas qui n'est pris ni
  par l'overlay des participants ni par celui des messages (l'app Tauri prenait le coin opposé à celui des participants).
- Galerie (#357, `apps/estompe/natif/src/replay/clips.rs`, `apps/estompe/natif/ui/replay/galerie.slint`, `Application::galerie`) : fenêtre « Clips » ouverte
  depuis le rail (bouton visible quand le replay est actif ou que le dossier contient des clips, comme l'espace de l'app Tauri) et depuis « Voir les clips » de
  la fenêtre « Instant replay ». Navigation : tous les clips ou un jeu (« Autres » sans jeu), place occupée et quota, ouvrir le dossier, réglages. Chaque clip :
  vignette, durée, nom, jeu et date, taille, définition, cadence, HEVC, piste micro, montage d'origine ; « Envoyer », « Renommer », « Supprimer » (confirmés),
  clic sur la vignette : lecture dans le lecteur du système (`Fichiers::ouvrir`) jusqu'au lecteur de l'app (lot 9). Lecture du dossier et vignettes sur un fil :
  une vignette manquante est tirée du clip par ffmpeg (`galerie::creer_vignette`, `.estompe/<clip>.mp4.jpg`), une seule tentative par clip. La galerie se relit
  à chaque `EvenementReplay::Clip` et à l'erreur `quota`. Règles portées de `galerie.rs` dans `estompe_client::replay::galerie` (`dans_dossier` : jamais hors du
  dossier ni à travers un lien, `renommer` et `supprimer` avec les annexes, `stockage`, `plus_anciens`, `purger`) et `estompe_client::replay::clips` (tri,
  jeux, textes, `decision_envoi`, `plan_envoi`). Quota : alerte à 90 %, puis « Quota du dossier atteint » avec « Supprimer les plus anciens… » : après
  confirmation, les clips les plus anciens sont supprimés jusqu'à repasser sous le quota, sauf les clips protégés (`Galerie::proteger`, pris par un envoi en
  cours et, demain, par l'éditeur) ; le clip en cours d'écriture est un `.part` des annexes, jamais listé ni supprimé. Rien n'est supprimé sans confirmation.
- Envoi d'un clip (#357) : « Envoyer dans un salon » liste les salons texte où l'on a `send_messages` et `attach_files` et les messages privés (recherche).
  Décision de l'app Tauri : MP4 H.264 à une piste envoyé tel quel ; deux pistes (jeu et micro) réunies par `amix` en copiant la vidéo ; HEVC converti en H.264
  par les encodeurs matériels des pièces jointes ; plus gros que `max_video_size` : réencodage plus léger proposé (débit et définition pour tenir dans la limite) ;
  trop long même réencodé : refus. La conversion (`preparer_fichier`, fichier temporaire de `<cache>/conversions/`) puis l'envoi en flux (`Client::envoyer_piece`)
  s'affichent dans la galerie avec leur pourcentage et « Annuler » ; la pièce part ensuite en message par `Store::envoyer` (échec visible et « Réessayer » dans
  la conversation). Fichier converti encore trop gros : la fenêtre se rouvre en mode réencodage.
- Éditeur de clips (#358, `apps/estompe/natif/src/replay/editeur/`, `apps/estompe/natif/ui/replay/editeur.slint`, `Application::editeur`) : branché par
  `Galerie::definir_editeur` (bouton « Modifier » de chaque clip) ; fenêtre « Éditeur de clips » qui remplace la galerie le temps du montage, puis la rouvre.
  Modèle et règles portés de `montage.ts` et `montage.rs` dans `estompe_client::replay::montage` : projets au même format que l'app Tauri
  (`.estompe/projets/<id>.json`, version 2, reprise de la version 1, suivi du renommage d'un clip), ligne de temps (segments, points de coupe, blocs rognés à
  l'image près, retirer et restaurer), vitesse et son coupé par segment, pistes « Jeu », « Micro » (volume 0 à 200 %, muet), plusieurs clips et leur ordre, rapport
  du cadre, annuler et rétablir (200 états), limites (20 clips, 30 minutes), plan à l'image près et graphe ffmpeg de l'export, validation du montage reçu.
  `SessionMontage` (état pur de l'éditeur ouvert, tête de lecture sur l'axe) applique les opérations et produit la vue ; chaque modification enregistre le projet
  par un fil d'écriture (une écriture à la fois, la dernière gagne). Les clips du montage sont protégés (`Galerie::proteger`) tant que l'éditeur est ouvert et pendant
  un export.
- Aperçu de l'éditeur : sans lecteur vidéo dans l'app (lot 9), l'image et la lecture passent par ffmpeg avec le même plan et le même graphe que l'export
  (`plan_apercu`, `arguments_apercu`) : image seule à la tête de lecture (`image_seule`, 360p au plus, recherche `-ss` par entrée puis `trim` en images),
  image précédente et suivante (`pas`), lecture de la sélection depuis la tête (`extrait`, 30 i/s, images RGBA lues sur la sortie de ffmpeg et cadencées par
  l'horloge ; son mixé par un second ffmpeg, joué par `SortieSon`). Formes d'onde tirées par ffmpeg (`onde`, 200 barres, `trace_onde` dans un `Path`).
- Export : choix de la résolution (cadre ou hauteur plus petite) et de la cadence, puis `montage::exporter` dans un fil (encodeurs matériels de
  `ffmpeg -encoders` puis `libopenh264`, `libx264`), fichier `<nom> (montage).mp4` avec son annexe (`origine`), clips d'origine ouverts en lecture seulement.
  Progression et « Annuler » dans l'éditeur et dans la galerie (`Galerie::commencer_travail`, `avancer_travail`, `finir_travail`, qui relit la galerie) ;
  l'annulation tue ffmpeg et supprime les fichiers partiels, purgés aussi au démarrage (`purger_partiels`). Liste « Montages » au-dessus des clips :
  rouvrir, renommer, supprimer (confirmation ; les clips et les fichiers exportés restent), clip manquant signalé.
- Écarts de l'éditeur avec l'app Tauri : pas d'édition des blocs de texte (gardés dans le projet, ni affichés ni incrustés à l'export, un message le dit) ni de
  cadre déplaçable à la souris (rapport choisi, cadre centré) ; pas de fichiers dérivés (piste `.m4a`, aperçu H.264) puisque l'aperçu lit le clip par ffmpeg ;
  aperçu à 30 i/s au plus en 360p ; glisser-déposer des segments remplacé par « vers le début », « vers la fin ».
- Garde-fous : pas d'encodeur matériel ou garde déclenchée : `indisponible` avec la raison ; encodeur qui accepte des images sans rien rendre en 2 s : `encodeur-muet` ;
  plus d'une image sur cinq perdue sur 10 s : alerte selon la cause ; aucune image du jeu dans les 3 s qui suivent l'ouverture de la capture : `capture-refusee` ; disque plein ou quota atteint à
  l'écriture : erreur, aucun clip existant n'est supprimé.
- Écarts avec l'app Tauri : pas d'AMF appelé directement ni de son essai hors processus (une carte AMD passe par son MFT, que l'app Tauri jugeait muet : c'est alors
  `encodeur-muet`) ; pas d'entrées D3D11 (réserve, textures) ni de conversion sur la carte graphique : l'image du jeu arrive en RGBA de la capture et passe en NV12 sur le
  processeur, l'encodeur la reçoit en mémoire vive ; pas de conversion HDR vers SDR ; vignette tirée par la galerie à la première ouverture plutôt qu'à l'écriture ; fil d'écriture en priorité
  normale.
- Tests : modules purs (`replay::*::tests` de la crate client, dont `ecriture::tests::mp4_de_synthese_lu_par_ffmpeg` : H.264 et AAC produits par ffmpeg, MP4 écrit par
  `ecriture`, relu par `ffmpeg -v error` sans erreur, trois pistes et 2 s ; sauté sans ffmpeg avec libx264) ; plateforme (`encodage::factice::tests`, `encodage::tests`,
  `disque`) ; service (`replay::tests` : jeu détecté, tampon armé, clip par la touche puis par `sauvegarder_les_dernieres`, pause et fermeture, indisponible sans
  encodeur, touche prise refusée, encodeur muet ou arrêté, échec pendant un partage d'écran mis en pause) ; réglages (`replay::reglages::tests`, `tests/replay.rs` :
  valeurs hors bornes ramenées, touches refusées) ; indicateur (`overlay::replay::tests` : placement, clip sauvé, erreur, expiration ; captures `overlay-replay-*`) ;
  captures de la fenêtre (`jeux-replay-*`) ; galerie (`replay::clips::tests` : vue vide, remplie, par jeu, quota presque atteint et dépassé, fenêtre d'envoi, destinations, préparation du fichier par ffmpeg ; `replay::galerie::tests` et `replay::clips::tests` de la crate client : chemins refusés, renommage et suppression avec les annexes, purge sans les clips protégés ni le `.part`, vignette ; `tests/clips.rs` : ouverture, purge confirmée, clip protégé refusé, renommage, suppression ; captures `clips-*`) ; éditeur (`replay::montage::*::tests` de la crate client : équivalents des cas de `montage.test.ts` et de `montage.rs`, dont les exports réels par ffmpeg,
  un export coupé à des instants donnés à une image près, l'annulation et l'aperçu à l'image près ; `replay::editeur::*::tests` : opérations de la session, vue,
  format d'export, suivi des exports, touches ; `tests/editeur.rs` : ouverture depuis la galerie, coupe, retrait, muet, annuler et rétablir au clavier, export
  puis montage listé et rouvert, clip d'origine inchangé ; captures `editeur-*` et `clips-montages-clair`).
- Non vérifié ici (machine Linux) : encodage Media Foundation réel, 30 minutes de jeu avec le tampon, images par seconde du jeu et CPU comparés à l'app Tauri (C3 de
  #355) ; indicateur « clip sauvé » sur un vrai jeu après le raccourci (C3 de #356) ; montage et export d'un clip réel de 2 minutes puis envoi (C4 de #358), son de
  l'aperçu sur une vraie sortie audio.

## Capture d'écran et de fenêtre

Le trait `CaptureEcran` de la crate plateforme (`apps/estompe/crates/plateforme/src/capture/`) sert au partage d'écran : sélecteur de source et
publication dans le salon (voir [Partage d'écran](#partage-décran)), incrustation de la caméra (lot 5), capture de la fenêtre d'Estompe pour un signalement (lot 8).
`EcransFactices` (même module) donne des sources factices aux tests : images poussées à la demande, fenêtre réduite, fermée ou en échec. Il ne fait ni interface
ni publication, ni son : celui du partage a son propre trait (`CaptureSon`, voir [Partage d'écran](#partage-décran)), qui reçoit le processus de la fenêtre (`SourceCapture::processus`).

- `sources()` : écrans puis fenêtres, chacun avec un identifiant (`ecran:<n>` ou `fenetre:<n>`, `IdSource`), un titre, l'application (nom de
  l'exécutable sous Windows, classe `WM_CLASS` sous X11), la taille, l'écran principal, le processus et `estompe` (fenêtre d'Estompe elle-même,
  reconnue à son processus, que le sélecteur peut écarter). Les fenêtres sans titre, réduites ou plus petites que 120 × 80 sont écartées, comme
  dans l'app Tauri. Les miniatures ne sont pas dans la liste : `miniature(source, hauteur)` les calcule une par une (`HAUTEUR_MINIATURE` = 180),
  pour que le sélecteur affiche la liste tout de suite.
- `capturer(source)` : une image RGBA, sans curseur.
- `demarrer(source, OptionsCapture { ips, curseur }, recepteur)` : flux continu sur un fil à part. Le récepteur reçoit `EvenementCapture` :
  `Trame(Image)` (RGBA, alpha à 255), `Reduite` puis `Restauree` (une fois chacun, rien n'est envoyé entre les deux), `Fermee` ou `Echec`
  (dix échecs de suite), qui terminent le flux ; il renvoie `false` pour l'arrêter. Le `FluxCapture` rendu change la cadence
  (`definir_cadence`, de 1 à 240 images par seconde) et le curseur (`definir_curseur`) pendant le flux, et l'arrête quand il est lâché ou par
  `arreter()`, y compris depuis le récepteur.
- Cadence : `Cadence` (reprise de `capture_fenetre.rs`) laisse passer une image par intervalle, tolère une image en avance d'un quart
  d'intervalle et borne le rattrapage après un silence. Les parties pures (cadence, conversion BGRA vers RGBA, réduction des miniatures,
  superposition du curseur) sont testées sur toutes les plateformes.

| | Windows | Linux X11 | Wayland, macOS |
|---|---|---|---|
| Liste | `xcap` | `_NET_CLIENT_LIST`, RandR (`x11rb`) | indisponible (portails au lot 11) |
| Écran en continu | Windows Graphics Capture (`xcap`, enregistreur vidéo), curseur dessiné par GDI | `GetImage` sur la racine, curseur par XFixes | |
| Fenêtre en continu | Windows Graphics Capture (fenêtres accélérées par le GPU comprises), curseur par WGC ; repli image par image (`xcap`) | Composite (`NameWindowPixmap`), sinon `GetImage` sur la fenêtre ; curseur par XFixes | |
| Fenêtre réduite, fermée | `IsIconic`, `IsWindow` | état `_NET_WM_STATE_HIDDEN` ou fenêtre non affichée ; fenêtre détruite | |

- Linux : `xcap` n'est pas utilisé, il impose PipeWire (bibliothèque du système à la compilation) et passe par Wayland ou les portails ; `x11rb`,
  déjà là pour les raccourcis globaux, suffit sous X11 sans dépendance du système. `disponible()` est faux sous Wayland, sans `DISPLAY`, ou si le
  serveur X n'a pas de pixels de 32 bits en ordre little-endian.
- Tests sous X11 (`capture::x11::tests`, ignorés sans serveur X, par exemple `xvfb-run -a cargo test -p estompe-plateforme capture`) : liste et
  capture de l'écran, flux de l'écran à 30 images par seconde, fenêtre d'essai (titre, application, processus, pixels), flux à 30 images par
  seconde, puis réduction, restauration et fermeture signalées ; la partie fenêtre n'est jouée que sans gestionnaire de fenêtres (le test tient
  lui-même `_NET_CLIENT_LIST`).
- Tests sous Windows (`capture::windows::tests`) : liste et capture du premier écran, flux de l'écran ; ignorés si le poste n'a pas d'écran
  capturable, obligatoires avec `ESTOMPE_CAPTURE_EXIGE=1`.
- Non vérifié sur machine réelle : capture d'un écran, d'un navigateur et d'un jeu fenêtré sous Windows à 30 et 60 images par seconde, cadence mesurée.

## Portabilité

Aucun `cfg(target_os)`, `cfg(windows)` ni `cfg(unix)` hors de `apps/estompe/crates/plateforme` : `scripts/estompe/natif-portabilite.sh` le vérifie
(seul `windows_subsystem = "windows"` est admis, pour ne pas ouvrir de console sous Windows).

La crate plateforme expose un trait par fonction : coffre de secrets, notifications, pastille de la barre des tâches, zone de notification,
installation d'une mise à jour, raccourcis globaux, ouverture d'une URL, capture d'écran et de fenêtre, capture du son, overlay, application au
premier plan, encodage vidéo matériel, démarrage avec la session. Chaque trait a une méthode `disponible()`, que
l'interface consulte pour masquer ce qui n'existe pas, et l'implémentation `Indisponible`, qui répond `Erreur::Indisponible` sans paniquer.
`Plateforme::de_l_os()` assemble ce que l'OS sait faire. Sont implémentés sur les deux OS : les raccourcis globaux de l'appuyer pour parler (`RaccourcisSysteme` : crochet clavier sous Windows, XInput 2 sous X11, indisponible sous Wayland), le coffre de secrets (`keyring` : Credential Manager
sous Windows, Secret Service sous Linux) l'ouverture d'une URL `http` ou `https` dans le navigateur, le choix d'un dossier ou de fichiers et l'ouverture d'un fichier (`Fichiers` : `rfd` pour le
sélecteur, `explorer` ou `xdg-open` pour l'ouverture), la copie d'une image et la lecture d'une image ou d'une liste de fichiers (`PressePapiers`, `arboard`), les périphériques audio (`SystemeAudio`, `cpal` : WASAPI sous Windows, ALSA sous Linux ; `SonPeripheriques` pour le micro et la sortie des messages vocaux), le lancement d'un processus d'arrière-plan
(`commande_d_arriere_plan`, utilisé pour ffmpeg), les compteurs du processus (`MesureProcessus` : mémoire et temps CPU, processeur, mémoire de la machine, système), la zone de notification (`ZoneNotification`, `tray-icon`), les notifications (`Notifications` : toasts sous Windows, `notify-rust` sous Linux) et
l'instance unique (fonctions de `instance.rs`, `interprocess`). La capture d'écran et de fenêtre (`CaptureSysteme`, voir [Capture d'écran et de fenêtre](#capture-décran-et-de-fenêtre)) l'est sous Windows et sous X11. Sous Windows seulement : le démarrage avec la session, la pastille de la barre des tâches, l'installation d'une mise à jour (`InstallationSysteme`), l'application au premier plan (`PremierPlanSysteme`, voir [Jeu au premier plan](#jeu-au-premier-plan)), l'overlay (`OverlaySysteme`, voir [Overlay](#overlay)) et l'encodage vidéo matériel (`EncodageSysteme`, voir [Instant replay](#instant-replay)). Le reste arrive avec son lot, Windows d'abord.

Les tests du coffre du système sont ignorés (et le disent) sur une machine sans coffre ; `ESTOMPE_COFFRE_EXIGE=1` les rend obligatoires, comme en CI.
De même, les tests qui ont besoin du bus de session D-Bus (zone de notification, faux serveur de notifications sous Linux) sont ignorés sans bus, et obligatoires avec `ESTOMPE_DBUS_EXIGE=1` (job `estompe-natif-linux`).

## Données locales

Identifiant `fr.plbls.estompe.natif`, distinct de `fr.plbls.estompe` et `fr.plbls.estompe.test` de l'app Tauri : aucun fichier n'est partagé.

| | Windows | Linux |
|---|---|---|
| Données (`preferences.json`) | `%APPDATA%\fr.plbls.estompe.natif` | `$XDG_DATA_HOME/fr.plbls.estompe.natif` (défaut `~/.local/share`) |
| Cache (avatars dans `avatars/`, images dans `medias/`, vidéos converties dans `conversions/`, installeurs téléchargés dans `mises-a-jour/`) | `%LOCALAPPDATA%\fr.plbls.estompe.natif\cache` | `$XDG_CACHE_HOME/fr.plbls.estompe.natif/cache` (défaut `~/.cache`) |
| Journaux (`estompe-debug.log`, `estompe-debug.1.log`) | `%LOCALAPPDATA%\fr.plbls.estompe.natif\journaux` | `$XDG_DATA_HOME/fr.plbls.estompe.natif/journaux` |
| Coffre de secrets | service `fr.plbls.estompe.natif` | service `fr.plbls.estompe.natif` |
| Instance unique | tube nommé `\\.\pipe\fr.plbls.estompe.natif-<empreinte>` | `$XDG_RUNTIME_DIR/fr.plbls.estompe.natif/instance.sock` (sinon `<données>/instance.sock`) |

- `ESTOMPE_NATIF_DOSSIER=<dossier>` place tout sous `<dossier>/{donnees,cache,journaux}` : une seconde instance, ou un essai sans toucher au profil.
- Préférences : thème, densité, réduction des animations, notifications, aperçus, journal de débogage, fonctions alpha débloquées, dernière catégorie des paramètres, ordre des serveurs, géométrie de la
  fenêtre (retenue à la demande de fermeture et en quittant, tant que la fenêtre est visible et pas réduite : une fois masquée, sa position n'est plus connue ; une position hors de ±16 384 px, comme celle d'une fenêtre réduite sous Windows, n'est pas réappliquée), adresse du serveur, dernière vue (compte et adresse `#/s/…` ou `#/dm/…`), panneau replié, marge ouverte, micro et sortie audio choisis (`micro`, `sortie_audio` : identifiant `cpal`, absent = périphérique par défaut), chaîne du micro (`traitement_micro`), écoute du vocal (`ecoute`), appuyer pour parler (`ptt` : actif, touche, délai) et sons du vocal (`sons_vocal`, `sons` : son d'arrivée, son de départ, volume ; les fichiers personnalisés sont à part, dans `<données>/sons/`), bornés à la lecture, smileys récents (`emoji_recents`), dernières recherches (`recherches_recentes`), messages vocaux écoutés (`vocaux_ecoutes`, 500 au plus). Écriture atomique ; un fichier illisible est mis de côté (`preferences.json.corrompu`) et recréé avec les valeurs par défaut.
- Adresse du serveur : `https://chat.plbls.fr` par défaut. Pour un serveur de test, la variable `ESTOMPE_API_URL`, sinon la préférence
  `adresse_serveur` ; une adresse invalide est écartée et notée dans le journal.
- Journal de débogage : écrit seulement si la préférence `journal_debogage` est activée, tournant à 512 Ko, exportable. Jetons, `Bearer` et
  paramètres secrets y sont masqués avant écriture. Il se consulte, s'exporte et se vide dans la catégorie « Débogage » des [paramètres](#paramètres).

## Paramètres

Page de la zone de contenu, comme `SettingsModal.svelte` (une page, pas un modal) : `ui/parametres/page.slint` (global `DonneesParametres`), `src/parametres.rs`
(préférences locales, journal, alpha), `src/coquille/pont/parametres.rs` (réglages enregistrés sur le compte, aperçu des liens), logique pure dans
`estompe_client::parametres` (catégories, index de recherche, raccourcis, déblocage des fonctions alpha) et `estompe_client::journal` (lecture, filtre et format des lignes).

- Ouverture : bouton « Paramètres » de la barre de l'utilisateur, entrée « Paramètres » de son menu ou action « Paramètres › <catégorie> » de la palette (`>`) ; la catégorie rouverte est la dernière choisie
  (préférence `onglet_parametres`), sinon « Compte ». Comme le front, toute navigation (salon, accueil, palette) referme la page ; la marge est masquée tant qu'elle est ouverte.
- Catégories : celles du front dans son ordre (compte, notifications, voix et vidéo, apparence, application, débogage, signalements), puis « Raccourcis »
  (propre à l'app native) et « Instant replay » (bêta, seulement s'il est disponible, comme la page de l'app Windows). La recherche ignore accents et casse et
  liste sous chaque catégorie les réglages trouvés (`chercher`, mêmes cas que `settings.test.ts`).
- Clavier : la catégorie choisie a le focus à l'ouverture ; flèches haut et bas (en boucle), Origine et Fin passent d'une catégorie à l'autre, Tab entre dans la page,
  Ctrl+F va à la recherche, Échap efface la recherche puis ferme la page et rend le focus à la coquille. Un modal ouvert depuis la page (réglages audio, jeux, replay, mesures)
  la rend inactive ; à sa fermeture le focus revient à la catégorie.
- Préférences synchronisées comme dans le front : les réglages de notification (voir ci-dessous) sont enregistrés sur le compte (`PUT /me/notify-settings`, retour en
  arrière et message si l'envoi échoue) et suivis en temps réel (`notify_settings.updated`) ; tout le reste est local (`preferences.json`) : thème, densité, aperçu des liens,
  réduction des animations, notifications du bureau, lancement avec la session, mode debug, fonctions alpha.
- Notifications (`SettingsNotifications.svelte`, logique pure dans `estompe_client::niveaux_notification`) : notifications du bureau (local), « Ne pas déranger » (le statut,
  comme le menu de la barre), niveau général, types, aperçu et son, puis un serveur (celui ouvert, sinon le premier ; « Changer de serveur… » ouvre un menu) : son niveau, sa
  sourdine, « Ignorer @everyone et @here », ses salons texte réglés (niveau ou sourdine) et « Régler un autre salon… » (menu, le salon ajouté passe à « Tous »). Chaque choix
  « Par défaut » ou « Comme le serveur », « Tous », « Mentions », « Rien » affiche le niveau qui s'applique et sa source (`niveau_effectif`, mêmes règles que le serveur).
  Chaque changement est appliqué au store tout de suite (`Store::appliquer_reglages_notification`) puis envoyé : deux changements rapides se cumulent. La page est reprojetée
  quand les réglages, mon statut, les serveurs ou les salons changent, à l'ouverture de la catégorie et toutes les 30 s tant qu'une sourdine existe.
- Règles des niveaux (décidées par le serveur, `blocks/notifications/src/decision.rs`, reprises par le client pour l'affichage) : une sourdine active du salon, de la conversation
  privée ou du serveur coupe tout ; sinon le niveau du salon, de la conversation privée, puis du serveur ; sinon le niveau général (`general`) ; sinon mentions seulement en salon
  et tout en privé. En privé, le niveau général ne compte que s'il vaut « Rien ». Sourdine (`mutes` : identifiant → fin en millisecondes, 0 = jusqu'à réactivation) : 15 min, 1 h,
  8 h, 24 h ou jusqu'à réactivation ; échue, elle ne compte plus (le serveur l'ignore et la retire au prochain enregistrement), sans action de l'app. Fin lue en heure locale
  (« jusqu'à 10:15 », « demain 09:30 », « le 22/11 à 09:30 »).
- Menus contextuels : « Mettre en sourdine… » d'un salon texte (panneau) ou d'un serveur (rail : clic droit, touche Menu ou Maj+F10, absent du front) ouvre à la place le menu
  des durées (`menu::choisir` laisse ouvert le menu qu'une action vient d'ouvrir) ; en sourdine, l'entrée devient « Réactiver les notifications ». Le menu d'un serveur offre aussi
  « Paramètres de notification » (la catégorie s'ouvre sur ce serveur) et « Copier le lien ».
- Apparence : le thème choisi s'applique tout de suite (`Theme.mode`), comme le bouton de l'en-tête ; la densité compacte resserre les messages (`Affichage.compacte`) ;
  réduire les animations met les durées à zéro (`Mouvement.reduit`).
- Débogage : 500 dernières lignes du journal (relu toutes les 3 s tant que la catégorie est ouverte), filtre par niveau et par texte, copie de tout le journal, export
  dans le dossier des téléchargements (`estompe-debug-AAAA-MM-JJ-HH-MM-SS.log`, heure locale), vidage ; « Mesures de performance… » quand le mode debug est actif.
- Application : « À propos », panneau des mises à jour, lancement à l'ouverture de la session ; 7 clics sur « Version installée » basculent les fonctions alpha
  (préférence locale `alpha`, comme `UpdateSettings.svelte`).
- Compte : voir [Profil](#profil).
- Encore ailleurs : réglages audio, « Jeux et activité » et instant replay restent des fenêtres ouvertes depuis leur catégorie (#402) ; les signalements (#367) arrivent avec leur issue. Écarts des notifications : pas de notification push de test (push web : écart assumé), aucune marque de sourdine
  dans la liste des salons ni sur le rail (le front n'en a pas).

### Profil

Catégorie « Compte » (`ui/parametres/profil.slint`, global `DonneesProfil` ; `src/coquille/pont/profil.rs` ; images dans `src/coquille/profil.rs` ; calculs purs dans
`estompe_client::profil`), comme le compte de `SettingsModal.svelte` et le menu de `ProfileModal.svelte` :

- Avatar : « Changer l'avatar » ouvre le sélecteur de fichiers de l'OS (PNG, JPEG, WebP, GIF ; 32 Mo au plus, réduite à 2 048 px à l'ouverture), puis la fenêtre
  « Recadrer l'avatar » ; « Supprimer » (s'il y en a un) envoie `DELETE /me/avatar`. Mon avatar est aussi celui de la barre de l'utilisateur.
- Photos par serveur : une ligne par serveur avec la photo qui s'y applique (la sienne, sinon la globale), « Changer » (même recadrage, `PUT /servers/{id}/me/avatar`)
  et « Retirer » si le serveur en a une (`DELETE /servers/{id}/me/avatar`).
- Recadrage (`Recadrage`, `ui/composants/recadrage.slint`, réutilisé pour l'icône d'un nouveau serveur) : aperçu carré de 240 px aux coins de l'avatar (partie de l'image par
  `source-clip`), glisser ou flèches pour déplacer (12 px), curseur de zoom de 1 à 4 (pas de 0,05) ou + et - (0,25). Mêmes calculs que `cropRect` du front : `zoom` réduit le
  carré source, `x` et `y` dans [-1, 1] le placent d'un bord à l'autre ; un glisser déplace l'image dans le sens du pointeur. « Valider » envoie un carré de 256 px en PNG,
  en JPEG (qualité 85) s'il dépasse 200 Ko, comme `squareAvatar` ; le front centre l'avatar sans recadrage, l'app native recadre l'avatar et les photos par serveur.
- Bannière : « Couleur du rôle » ou une des dix nuances (`NUANCES_BANNIERE`), `PUT /me/banner` ; le front propose un sélecteur de couleur libre dans le menu de la carte,
  l'app native une palette fixe (pas de sélecteur de couleur dans Slint).
- Rien n'est appliqué d'avance : `avatar.updated`, `server_avatar.updated` et `banner.updated` mettent la page, la barre, la liste des membres et la carte à jour, chez soi
  comme chez les autres. Un échec affiche « Image impossible à utiliser comme avatar. » (ou « … comme photo de serveur. »), « Photo non supprimée. » ou « Couleur non enregistrée. ».
- Pas dans le front, donc pas dans l'app native : pseudo par serveur choisi par soi-même (seulement par la modération, `PUT /servers/{id}/members/{uid}/nickname`), bio,
  statut personnalisé, choix des badges affichés (aucune route).

## Types de l'API

`apps/estompe/crates/client/src/genere/types.rs` est généré depuis `docs/estompe/openapi.yaml` par `scripts/estompe/gen-types-client.mjs`.
Après un changement du contrat : `just estompe-natif-generer`. La CI échoue si le fichier versionné ne correspond plus (`--check`).

## Captures de rendu

`apps/estompe/natif/tests/captures.rs` rend chaque écran hors écran avec le moteur logiciel de Slint (backend de test, rendu en Rust pur,
identique d'une machine à l'autre), à 1180 × 900, avec des données fixes, le temps figé et les animations coupées, puis le compare à `tests/references/<écran>.png`. Un pixel
compte s'il s'écarte de plus de 24 sur un canal ; le test échoue au-delà de 0,1 % de pixels différents. En cas d'échec, la capture obtenue, la
référence et l'image des différences sont dans `target/tmp/captures/`, publiées par la CI dans l'artefact `estompe-natif-captures`.

- Ajouter un écran : une entrée dans `ECRANS` (nom, thème, fonction qui prépare la fenêtre), puis `just estompe-natif-captures` et versionner l'image.
  Les overlays ont leurs propres captures (`tests/overlay.rs`, régénérées par la même recette, mêmes seuils et même variable `ESTOMPE_CAPTURES`, fonctions communes dans `tests/commun/mod.rs`, module `references`).
- Mises à jour : `mise-a-jour-bandeau`, `mise-a-jour-panneau-a-jour`, `mise-a-jour-telechargement` (40 %), `mise-a-jour-erreur` et `mise-a-jour-forcee`
  (installation en cours) sont préparés par `mise_a_jour::afficher` avec un `EtatMaj` fixe.
  Les écrans d'entrée sont préparés par `ecrans::afficher` avec un `Etat` fixe ; la coquille par un `Pont` sur un store rempli par `Store::installer`
  (`tests/commun/mod.rs`, module `coquille`), l'heure figée et une icône de serveur dessinée dans le test.
- Régénérer après un changement voulu : `just estompe-natif-captures`, relire les images, les versionner dans la même PR.
- Les captures ne sont comparées que sous Linux (`ESTOMPE_CAPTURES=ignorer` sur le runner Windows).
- Le moteur Skia n'est pas utilisé ici : son rendu du texte dépend des bibliothèques du système, et les mêmes écrans diffèrent d'environ 1 % de
  pixels entre deux machines. Écarts du moteur logiciel avec ce que l'app affiche (Skia) : pas d'ombre portée sous les modals et les cartes, un point tracé par
  un segment de longueur nulle n'est pas dessiné (icônes `poignee` et `list`), anticrénelage du texte légèrement différent. Il ne découpe pas une image
  par le `border-radius` de son parent : les icônes des serveurs sont recouvertes par `Coins`, dans les deux moteurs, et les avatars arrondis dans leurs pixels ;
  un rayon propre à un coin (`border-top-left-radius`) n'est pas dessiné.

## Tests d'interaction

Backend de test de Slint, un par fichier de `tests/` : `clavier.rs` et `connexion_clavier.rs` sans boucle d'évènements (`init_no_event_loop`),
`connexion.rs` et `coquille.rs` avec la boucle (`init_integration_test_with_mock_time`, un seul `#[test]` par fichier) : un fil pilote clique par `ElementHandle` et attend
chaque état. En temps simulé, la boucle du backend de test ne lance pas les gestionnaires `changed` (la boucle winit le fait à chaque tour) : `sur_l_interface`
(`tests/commun/`) les lance après chaque action (`mock_elapsed_time` d'une durée nulle), sinon un compteur comme `Calques.ouverts` resterait en retard d'une fenêtre. `coquille.rs` branche la coquille sur une fausse fabrique (store réel rempli par `Store::installer`, `FluxTempsReel::factice`) :
un non-lu ou une mention reçus en temps réel apparaissent sur le rail sans action, puis navigation, préférences, bandeau du réseau, section et fin de session. `ElementHandle` exige les informations de débogage de l'interface, que `build.rs` n'ajoute qu'au profil de débogage.
`membres.rs` (même backend) ouvre un serveur de 5 000 membres sur une fausse source de profils qui compte les requêtes : seules les lignes de la fenêtre
  sont instanciées et demandent leur avatar, un défilement ne demande que les nouvelles, un retour en arrière aucune ; un statut ou un passage en ligne reçus en temps réel
  ne remplacent ou ne déplacent que la ligne du membre. Sans rendu, le test fait tourner lui-même les gestionnaires `changed` (`update_timers_and_animations`).
`messagerie.rs` (même backend, faux serveur HTTP de 300 messages, `ServeurMessages` de `tests/commun/`, qui sert aussi des fichiers) : ouverture sur le premier non-lu, saisie, message reçu en lisant l'historique sans défilement, modification, épinglage et suppression par une seule opération sur le modèle, liens refusés, saut vers un message cité non chargé, non-lu fenêtre inactive puis lu au retour, lecture sur un autre appareil ;
  puis une image de 8 000 × 8 000 (décodée hors du fil d'interface, qui répond pendant ce temps, plafond mémoire respecté), un GIF animé à l'écran puis
  hors de l'écran (aucun réveil ni image en 10 s simulées), le téléchargement d'un fichier au nom hostile dans un dossier temporaire puis son ouverture, la
  visionneuse (navigation, zoom), un vote et une réaction reçus en temps réel.
`saisie.rs` (même backend, `ServeurMessages` qui accepte aussi `POST …/messages` et `PATCH /messages/{id}`) : saisie au clavier, Maj+Entrée, envoi par Entrée et message du serveur
  en bas, flèche haut et Échap, modification, refus 403 puis « Réessayer », mentions (`@`, Tab), salons (`#`, Échap), smileys (`:`, flèches, Entrée), conversion de `:thumbsup:`,
  sélecteur d'emoji ouvert, parcouru et refermé au clavier (récents écrits dans les préférences), brouillon retrouvé en changeant de salon et relu depuis le disque ;
  sélecteur de GIF (tendances à l'ouverture, focus sur la recherche, 10 caractères tapés en 500 ms simulées : une seule recherche, seuls les GIF à l'écran animés et plus le premier
  après défilement, suite au défilement, choix au clavier, import puis envoi en réponse, 429 sans nouvelle tentative, sélecteur gardé dans une vue étroite) ;
  actions sur un message (clic droit sur un message, menu par la touche Menu et Maj+F10, répondre, épingler, réaction ajoutée par le sélecteur puis retirée par Entrée, copier le texte
  et le lien, marquer comme non lu, supprimer confirmé au clavier, modifier), menu d'un salon (« Marquer comme lu ») et d'un membre (« Mentionner ») ;
  recherche et palette (Ctrl+K, portée du salon ouvert, `GET /search` vérifié, terme surligné, résultat ancien ouvert : page chargée autour et message mis en évidence, dernière
  recherche retenue ; palette : salon et conversation privée ouverts au clavier, flèches en boucle, Tab qui complète un filtre, Échap, erreur du serveur affichée) ;
  statut choisi par la palette (`>`) puis par le menu de la barre de l'utilisateur (corps du `PUT /me/status` vérifié), nouveau message privé vers une personne déjà en conversation
  (pseudo proposé et choisi au clavier, conversation existante ouverte sans doublon), refus `user_not_found` affiché, « Tout marquer lu » sur l'accueil (pastilles à zéro, `POST …/read`) ;
  sondage (bouton « Créer un sondage », erreurs de validation au clavier, choix ajouté puis retiré, échéance passée refusée, corps du `POST` vérifié, vote affiché tout de suite
  puis totaux de `poll.voted`, votants affichés puis masqués, clôture, suppression confirmée) ;
  puis les pièces jointes (`POST /files` du même serveur, qui peut refuser ou bloquer un envoi) : choix de deux fichiers et aperçu, fichier de 11 Mo refusé sans requête, envoi avec `file_ids`,
  échec puis « Réessayer », collage d'un fichier dont l'envoi est annulé (connexion fermée côté serveur, aucun message), collage d'une capture, dépôt sur la fenêtre, message sans
  texte, retrait d'une pièce d'un message modifié (`PATCH` avec la liste finale) ; puis les messages vocaux (micro et sortie simulés par `FauxMicro` et
  `FausseSortie` de la fausse fabrique : micro refusé puis absent, 1,5 s à 44,1 kHz avec durée et niveau, pause et réécoute à 440 Hz, envoi `kind=voice` relu par
  le décodeur, puis lecture d'un WebM du web et du message envoyé, vitesse, position, un seul à la fois, fin, fichier illisible) ; puis, si ffmpeg (avec `libx264` et `libx265`) est installé, une vidéo HEVC de synthèse
  jointe, envoyée en `kind=video` et reçue en H.264, fichier converti supprimé après l'envoi, puis une conversion (ou son envoi bloqué) annulée sans laisser de fichier.
  Les tests de `estompe_client::video` convertissent aussi une vidéo HEVC (`libx264` à la place des encodeurs matériels, absents du runner) et l'annulent pendant l'encodage.
  Sans ffmpeg, ces tests disent « ignoré » ; `ESTOMPE_FFMPEG_EXIGE=1` les rend obligatoires, comme en CI Linux.
`messagerie_volume.rs` : salon de 50 000 messages remonté pas à pas (lignes instanciées, modèles et mémoire bornés) ; un message repère garde sa position à l'écran quand une page plus ancienne arrive.
  Par défaut 8 pas de 1 400 px (8 s simulées, au moins 3 pages anciennes après le premier quart, environ 22 s en debug) ; `ESTOMPE_VOLUME=long` (ou `just estompe-natif-volume`) rejoue le parcours long de 120 pas de 1 400 px (2 minutes simulées, au moins 10 pages).
`jeux.rs` (même backend, `ServeurMessages`, `PremierPlanFactice` sondé toutes les 20 ms) : jeu détecté affiché sous mon pseudo, rien publié hors ligne, activité vide
  publiée sans le réglage, « Jeux et activité » ouvert depuis le menu de l'utilisateur, partage activé (`PUT /me/activity` avec le jeu), jeu exclu retiré de l'affichage et de
  l'activité publiée, application fenêtrée non détectée puis ajoutée comme jeu, exécutable exclu par « Parcourir… ».
`parametres.rs` (sans boucle d'évènements) : catégories dans l'ordre du front, flèches, Origine et Fin, thème, densité et animations réglés au clavier et enregistrés,
  Échap qui ferme et rend le focus à la coquille, catégorie retrouvée à la réouverture, recherche par Ctrl+F effacée par Échap, journal activé, filtré, exporté et vidé
  au clavier, déblocage des fonctions alpha. `coquille.rs` vérifie aussi qu'un type de notification changé sur un autre appareil (`notify_settings.updated`) est reflété
  dans la page ouverte, et qu'une navigation la referme. Profil : une image de synthèse recadrée au clavier (+, flèches) et à la souris (glisser) est envoyée
  (`PUT /me/avatar`) au faux serveur, qui la sert ensuite ; un autre client qui reçoit `avatar.updated` la télécharge et l'affiche dans sa liste des membres (pixel vérifié),
  ma page et ma barre aussi ; bannière envoyée puis reçue (`banner.updated`), photo d'un serveur retirée, « Modifier mon profil » de ma carte ouvre le compte. Le calcul du
  recadrage a ses tests unitaires (`estompe_client::profil`, `coquille::profil` : image en quadrants, pixel attendu).
`creation.rs` (même backend que `coquille.rs`, `ServeurMessages` qui accepte aussi `POST /servers`, `POST /server-invites/{code}/join`, `GET /servers`,
  `POST /servers/{id}/channels`) : sans `manage_channels` (droits changés dans le store), ni « + » ni « Créer un salon » dans les menus et la fenêtre ne s'ouvre pas,
  le droit rendu les fait revenir ; nom déjà pris, vide puis refusé par le serveur affichés ; salon texte privé (rôle, membre), sujet et mode lent envoyés, ouvert et listé ;
  un `channel.created` d'un autre membre l'ajoute au panneau en temps réel ; serveur créé avec une icône choisie et recadrée (`PUT /servers/s9/icon`) puis ouvert ;
  code invalide signalé, lien complet rejoint ; serveur quitté depuis ses réglages après confirmation. Les règles de `channel-create.svelte.ts` ont leurs tests unitaires (`estompe_client::creation`).
`reglages_serveur.rs` (même backend, `ServeurMessages` qui garde aussi invitations, bannis et emoji) : onglets selon les droits changés dans le store, nom enregistré
  (`PATCH /servers/s1`), lien créé avec expiration et utilisations, copié puis révoqué, emoji trop grand ou trop lourd refusé sans aucun envoi, emoji valide ajouté avec
  son nom proposé, renommé puis supprimé, membres cherchés sans accents, ban levé, serveur supprimé après la saisie exacte de son nom et page refermée. Les règles ont
  leurs tests unitaires (`estompe_client::reglages_serveur`).
`roles.rs` (même backend, `ServeurMessages` qui garde aussi le détail du serveur, ses rôles et les surcharges) : sans être créateur, un rôle au niveau du mien ou
  au-dessus reste en lecture seule et rien n'est envoyé ; rôle renommé, coloré et doté d'une permission (`PATCH` vérifié), créé, déplacé par un vrai glisser-déposer puis par
  une flèche, sans passer au-dessus du mien, supprimé après confirmation ; membre ajouté par la recherche puis retiré ; retirer « Rejoindre » à « @tout le monde » demande
  une confirmation (refusée puis acceptée) ; surcharges d'un salon en trois états par rôle, confirmation avant de se retirer la vue du salon ; enfin un autre client change
  les permissions : sur `roles.updated`, la zone de saisie passe en lecture seule et les onglets « Salons » et « Rôles » disparaissent. Les règles ont leurs tests unitaires
  (`estompe_client::gestion_roles`, équivalents des cas de `roles.test.ts` sur la modification).
`moderation.rs` (même backend, `ServeurMessages` qui répond aussi aux routes de modération) : menu d'un membre selon les permissions et le rang (rien sur la
  créatrice ni sur un rang égal), exclusion levée directement, avertissement (raison obligatoire), bannissement d'une durée personnalisée avec purge, pseudo borné à
  32 caractères puis envoyé, expulsion depuis la carte de profil ; salon renommé (nom déjà pris refusé), mode lent et verrouillage enregistrés, 5 derniers messages
  supprimés après confirmation et nombre affiché ; journal chargé puis filtré ; salon supprimé depuis les réglages ; enfin `member.timeout` me visant recharge le détail
  sans `send_messages` et la zone de saisie passe en lecture seule avec la fin de l'exclusion (#366 C2). Les règles ont leurs tests unitaires (`estompe_client::moderation`).
`mise_a_jour.rs` vérifie l'entrée sans interface contre un faux serveur qui compte les requêtes.
`mises_a_jour.rs` (même backend, la boucle lancée deux fois) : vrai service de mise à jour sur une fausse source (`SourceMaj`) et une fausse installation ;
  le bandeau apparaît avec les notes, « Plus tard » le masque pour cette version (une version suivante le réaffiche), « Mettre à jour » affiche 40 %,
  rien n'est lancé avant la fin du téléchargement, puis l'installeur est lancé une fois et l'app quitte par `Fermeture` ; ensuite l'erreur d'une
  vérification manuelle dans le panneau, une mise à jour imposée qui mène à l'écran bloquant, un échec d'installation qui propose la page de
  téléchargement, et « Réessayer » qui installe sans ouvrir le navigateur.
`bureau.rs` (même backend, la boucle lancée deux fois) : fausse zone de notification (`tests/commun/`, module `bureau`) ; un message d'instance ramène la fenêtre masquée et
  transmet l'invitation, fermer masque la fenêtre en gardant la connexion temps réel (un message reçu arrive encore), clic et « Ouvrir » la ramènent,
  « Quitter » arrête la coquille, ferme le flux et retire l'icône ; sans zone, la demande de fermeture quitte. Entre les deux, avec de fausses
  notifications et une fausse pastille : une mention reçue met la pastille à 1 (barre des tâches et icône de la zone), une notification est affichée
  (le journal de débogage ne contient pas son contenu), « Marquer comme lu » envoie la lecture au faux serveur et efface la pastille, « Répondre » envoie
  le texte, le clic ramène la fenêtre sur la conversation, et rien n'est affiché pour la conversation ouverte dans la fenêtre active.
`instance.rs` (sans backend) : le test tient la première instance, lance le vrai binaire avec `--invitation` : il sort avec succès sans fenêtre et le message reçu porte ses arguments.

## Livraison

- Version : clé `natif` de `.version`, montée par la CI à la fusion d'une PR qui touche `apps/estompe/natif/`, `crates/client` ou
  `crates/plateforme` (`scripts/estompe/version.mjs`) ; `bump-version.mjs --apply` la recopie dans `Cargo.toml` et `Cargo.lock`, d'où
  `VERSION` (« À propos », en-tête `X-Estompe-Version`) et la version de l'installeur.
- Tag `estompe-natif-vX.Y.Z` : `.github/workflows/estompe-livraison-natif.yml` (détail dans [release.md](release.md)).
- Installeur NSIS `apps/estompe/natif/installeur/estompe-natif.nsi` → `Estompe-natif-beta-X.Y.Z-setup.exe` :
  - « Estompe natif (bêta) », par utilisateur sans droits d'administrateur (`RequestExecutionLevel user`), dans `%LOCALAPPDATA%\Programs\Estompe natif` :
    `estompe-natif.exe`, `ffmpeg.exe` (conversion des vidéos et lecture des messages vocaux AAC, voir [Pièces jointes à l'envoi](#pièces-jointes-à-lenvoi)), `estompe.ico` (icône des raccourcis et `IconUri` des toasts), `desinstaller.exe` ;
  - raccourci du menu Démarrer, et sur le bureau (case cochée par défaut, sautée en mode silencieux) ;
  - registre (`HKCU`) : `Software\EstompeNatif` (dossier), désinstallation `Software\Microsoft\Windows\CurrentVersion\Uninstall\EstompeNatif`,
    identité des toasts `Software\Classes\AppUserModelId\fr.plbls.estompe.natif` (`DisplayName`, `IconUri`, mêmes valeurs que l'app).
    L'app Tauri garde son dossier, sa clé de désinstallation et l'identifiant `fr.plbls.estompe` : les deux cohabitent.
  - mode silencieux de mise à jour, `/S /RELANCER` : attend que `estompe-natif.exe` soit libre (l'app a quitté ; 120 × 500 ms au plus, en
    interactif un message « Fermez Estompe natif » avec Réessayer), installe, puis relance l'app si `/RELANCER` est passé ; en cas d'échec, relance l'exe en place.
  - désinstallation (refusée tant que l'app tourne) : programme, `ffmpeg.exe`, icône, raccourcis, clés ci-dessus, valeur `EstompeNatif` de `Run` (démarrage
    avec la session) et cache `%LOCALAPPDATA%\fr.plbls.estompe.natif\cache` (mises à jour téléchargées comprises) ; garde les préférences et
    brouillons (`%APPDATA%\fr.plbls.estompe.natif`), les journaux et le coffre de secrets.
  - non signé Authenticode pendant la bêta : SmartScreen avertit à la première installation.
- ffmpeg : le workflow lance `fetch-ffmpeg.mjs` (`TAURI_ENV_PLATFORM=windows`) avant `makensis`, qui prend
  `apps/estompe/windows/src-tauri/binaries/ffmpeg.exe` (autre chemin : `-DFFMPEG=<chemin>`). Sous Windows, le script lit l'archive avec le `tar`
  du système (bsdtar), `unzip` n'y étant pas garanti.
- Contrôle (`scripts/estompe/natif-installeur-check.sh <installeur> <exe>`) : taille affichée et plafonnée (`TAILLE_MAX_MO`, 90 Mo : 41 Mo pour
  ffmpeg compressé, mesurés, plus les 45 Mo prévus pour l'app, pas encore mesurée sur un build release), contenu
  listé par `7z` (exe, `ffmpeg.exe`, icône, désinstalleur), exe sans référence à `WebView2Loader` ni `EmbeddedBrowserWebView`.
- Manifeste de mise à jour `natif.json` (`scripts/estompe/natif-manifeste.mjs`), servi par la release fixe `estompe-natif-beta` :
  `https://github.com/Plblsfr/plbls.fr/releases/download/estompe-natif-beta/natif.json`. Format de `latest.json` de Tauri plus la taille :
  `{ version, notes, pub_date, mandatory, force, minVersion, message, platforms: { "windows-x86_64": { url, signature, taille } } }` ;
  `url` vise l'installeur de la release versionnée (`…/releases/download/estompe-natif-vX.Y.Z/Estompe-natif-beta-X.Y.Z-setup.exe`, redirigé
  vers `objects.githubusercontent.com` ou `release-assets.githubusercontent.com`), `signature` est le fichier `.minisig` en base64 (clé
  `apps/estompe/crates/client/src/mise_a_jour/cle.pub`, commentaire de confiance `estompe-natif X.Y.Z`), `taille` en octets.
- Essai local de l'installeur sous Linux : `apt-get install nsis p7zip-full`, puis dans `apps/estompe/natif/installeur/`
  `makensis -DVERSION=0.1.0 -DEXE=<un exe quelconque> estompe-natif.nsi` (après `TAURI_ENV_PLATFORM=windows node ../../windows/scripts/fetch-ffmpeg.mjs`)
  et le contrôle sur le résultat.

## CI

Jobs `estompe-natif-linux` et `estompe-natif-windows` de `ci.yml` : voir [ci.md](ci.md).
