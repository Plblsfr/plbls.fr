# App native (Slint, sans webview)

« Estompe natif (beta) » est l'app de bureau écrite entièrement en Rust, sans webview, à côté de l'app Tauri qui reste la version stable.
Elle s'installe et tourne en même temps qu'elle. Ce qui lui manque encore est dans la [matrice de parité](natif-parite.md).
En place : la fenêtre, l'écran « À propos », la page de démonstration des composants, les écrans d'entrée (connexion, accès refusé, activation, invitation, mise à jour forcée) la coquille (rail des serveurs, salons, messages privés, accueil, navigation), la marge « Membres » (liste des membres, présence, activité, carte de profil) et la messagerie en lecture (messages des salons texte et des conversations privées, emoji, pièces jointes, aperçus de liens, sondages, réactions, temps réel, marquage comme lu) et le début de l'écriture (zone de saisie, envoi, modification, brouillons, mentions et emoji, pièces jointes, conversion d'une vidéo en H.264, actions sur un message et menus contextuels, sondages, GIF, messages vocaux, voir [Écriture](#écriture)), la recherche de messages et la palette (voir [Recherche et palette](#recherche-et-palette)), la nouvelle conversation privée, « Tout marquer lu », les contacts en ligne et le menu de statut (voir [Nouvelle conversation, accueil et statut](#nouvelle-conversation-accueil-et-statut)), l'intégration au bureau (instance unique, zone de notification, voir [Intégration au bureau](#intégration-au-bureau) ; notifications du système et pastille de mentions, voir [Notifications](#notifications)), les mises à jour signées avec leur interface (voir [Mises à jour](#mises-à-jour)) ; le reste de l'écriture arrive avec le lot 3.

## Architecture

| Dossier | Rôle |
|---|---|
| `apps/estompe/natif` | binaire `estompe-natif` : interface Slint (`ui/`), moteur de rendu Skia, branchement des préférences et du journal (`src/session.rs`), écrans d'entrée (`src/entree.rs`, `src/ecrans.rs`), coquille (`src/coquille/`), intégration au bureau (`src/bureau/`) |
| `apps/estompe/crates/client` | logique sans interface : types de l'API générés depuis `openapi.yaml`, client HTTP, adresse du serveur, préférences, journal, store, temps réel, routes, accès aux sections |
| `apps/estompe/crates/plateforme` | tout le code propre à un OS, derrière des traits ; dossiers de l'utilisateur |

Les trois paquets forment un workspace Cargo à part (`apps/estompe/natif/Cargo.toml`, un `Cargo.lock`, toolchain stable, `deny.toml` propre),
hors du workspace racine comme les apps Tauri. Slint est figé en `=1.18.1`, comme dans l'overlay natif de l'app Tauri.
L'arbre de dépendances ne contient ni `tauri`, ni `wry`, ni `webview2-com` : `deny.toml` les interdit.

## Construire et lancer

Prérequis sous Linux (Debian, Ubuntu) : `libfontconfig1-dev libfreetype-dev libasound2-dev libxkbcommon-x11-0` et un pilote OpenGL (Mesa). Sous Windows : la
toolchain MSVC. Les binaires de Skia sont téléchargés à la première compilation.

```sh
just estompe-natif-run            # build de débogage : le bouton « Composants » ouvre la page de démonstration
just estompe-natif-build          # profil release (taille, LTO) ; `just estompe-natif-build ci` pour le profil sans LTO
just estompe-natif-test
just estompe-natif-check          # tous les contrôles de la CI Linux
```

`estompe-natif --essai-de-lancement` ouvre la fenêtre, attend sa première image puis quitte : c'est le lancement fait par la CI sous Xvfb.
`estompe-natif --arriere-plan` démarre sans montrer la fenêtre si l'icône de la zone de notification est en place (lancement à l'ouverture de la session), sinon la montre.

## Interface

- `ui/genere/couleurs.slint` et `ui/genere/icones.slint` sont générés par `scripts/estompe/natif-da.mjs` depuis
  `web/estompe/src/styles/v3-jetons.css` (thèmes « Papier » et « Fusain ») et `web/estompe/src/components/Icon.svelte`. Après un changement de
  l'un ou de l'autre : `just estompe-natif-generer`. La CI échoue si les fichiers versionnés ne correspondent plus (`--check`).
- `ui/jetons.slint` : typographie, rayons, espacements et durées de [la direction artistique](direction-artistique.md) ; les trois polices
  variables (`polices/`, licence OFL) sont embarquées dans le binaire. Tout texte de l'app utilise l'une d'elles (le code en ligne compris, en
  JetBrains Mono) ; seuls les caractères absents des trois polices (écritures non latines, emoji hors du corps d'un message : pseudo, citation de réponse) passent par une police du système.
- `emoji/twemoji.bin` : les emoji en couleur (voir [Emoji](#emoji)), licences `emoji/CC-BY-4.0-twemoji.txt` et `emoji/MIT-emoji-data.txt`, citées dans « À propos ».
- `ui/composants/` : texte riche (`TexteRiche`, blocs de markdown), séparateur de date ou de non-lus (`Separateur`), bouton, champ de saisie, case, interrupteur, modal, infobulle, icône, avatar et pastille de présence, indicateur de chargement, zone défilante,
  liste virtualisée (`ListeVirtuelle`), compteur (pastille de non-lus ou de mentions), étiquette (« bientôt »), badge de rôle (« Bot », « Créateur »), élément de liste, état vide, coins arrondis d'une image,
  menu contextuel et confirmation (voir [Menus contextuels et confirmation](#menus-contextuels-et-confirmation)).
- Un composant interactif placé dans un modal reçoit `calque: 1` : tant qu'un modal est ouvert, ceux du dessous n'acceptent ni focus ni clic.
- L'écran « À propos » affiche la version et l'attribution exigée par la licence royalty-free de Slint (`AboutSlint`).

### Menus contextuels et confirmation

- Menu (`ui/composants/menu.slint`, global `DonneesMenu` ; `src/menu.rs`) : une seule instance, au-dessus de tout dans `ui/app.slint`. `menu::ouvrir(f, titre, ancre, entrées, action)`
  le remplit et le place : au curseur (`Ancre::Point`) ou sous un bouton (`Ancre::Element`, retourné au-dessus s'il manque de place en bas, `retourner`), toujours gardé dans la
  fenêtre (`placer`, marge de 8 px), comme `placeMenu` et `flipY` du front ; la hauteur est calculée en Rust (`hauteur`). Sans entrée, rien ne s'ouvre. Une entrée (`Entree`) a
  une icône et peut être séparée, dangereuse, cochée, porter un raccourci ou être marquée « bientôt » (affichée, sautée par les flèches, inactive).
- Clavier : flèches (en boucle), Tab, Origine, Fin, Entrée ou Espace, Échap. Un clic hors du menu, la molette ou la perte du focus le ferment. L'élément qui l'a ouvert retient
  `a-ouvert-menu` et reprend le focus à la fermeture, sauf si l'action l'a donné ailleurs (l'action rend `true` : champ de saisie, sélecteur, confirmation ; `focus-ailleurs`).
- Ouverture : clic droit, touche Menu ou Maj+F10 sur l'élément qui a le focus. Un `FocusScope` qui prend le focus au clic avale l'appui : la ligne d'un message a
  `focus-on-click: false` et se donne le focus dans `pointer-event`, sinon le premier clic droit ne ferait que la sélectionner.
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
- Invitation : `estompe-natif --invitation <lien ou code>` la rejoint dès le compte vérifié ; depuis la coquille, « Rejoindre un serveur » ouvre
  la saisie. Le code est pris à la fin d'un lien (`ABCDE-23456`). Les liens `estompe://` ouverts par le système ne sont pas encore pris en charge.

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
| Rail | `rail.slint` | accueil (non-lus et mentions des messages privés), serveurs dans l'ordre choisi (image du serveur, sinon initiales ; trait de non-lus, pastille de mentions), « Rejoindre un serveur », entrées des sections ouvertes |
| Panneau | `panneau.slint` | salons du serveur courant (texte, esquisses, vocaux, liste simple comme `SidePanel.svelte`), ou accueil et messages privés (bouton « Nouveau message privé »), ou section ; barre du compte (statut ou jeu, menu de statut) avec « Se déconnecter » ; repliable |
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
- Salons vocaux, cinéma, vidéos et musique sont listés avec l'étiquette « bientôt » ; leur vue l'explique. Un salon texte ou un message privé ouvre la vue des messages (voir [Messagerie](#messagerie)).
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
  d'origine, badges, statut et jeu, exclusion, rôles, dates d'arrivée sur le serveur et sur Estompe. Échap, le fond ou la croix la ferment. « Envoyer un message » et
  « Modifier mon profil » sont marqués « bientôt » ; modification, modération et historique ne sont pas dans l'app native.

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
- Sans image, les initiales restent affichées. Les images et emojis des badges ne sont pas encore chargés : une médaille de la couleur du badge les remplace.

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
  `estompe_plateforme::commande_d_arriere_plan` (sous Windows, sans fenêtre de console et en priorité basse). Le binaire à embarquer est celui de l'app Tauri
  (`apps/estompe/windows/scripts/fetch-ffmpeg.mjs`, build LGPL de BtbN) : 130 Mo installé, environ 40 Mo dans un installeur compressé en LZMA ; l'installeur natif
  ([#278](https://github.com/Plblsfr/plbls.fr/issues/278)) devra le copier à côté d'`estompe-natif.exe` et citer sa licence. La détection n'a lieu qu'à la première vidéo, hors du fil d'interface.
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

- Logique sans interface dans `estompe_client::vocal` (testée sans périphérique, sur des signaux de synthèse) : niveau en dBFS et forme d'onde de 40 barres
  (`niveau`, `forme_d_onde`, mêmes calculs que `levelOf` et `toWaveform` du front), rééchantillonnage linéaire, encodage Opus (`Encodeur` : 48 kHz mono,
  32 kbit/s comme `recorder.svelte.ts`, application VoIP, trames de 20 ms) dans un Ogg écrit à la main (`OpusHead`, `OpusTags`, une page par seconde, granule exact
  à la fin), décodage d'un Ogg Opus ou d'un WebM Opus (lecture EBML minimale, tailles inconnues de MediaRecorder comprises) ; tout autre format (AAC en MP4 de
  Safari) passe par ffmpeg s'il est présent (`decoder_avec_ffmpeg`), sinon le message est « illisible ». Opus vient de `unsafe-libopus` (libopus 1.3.1 traduit en
  Rust : aucune chaîne C à installer, ni sous Linux ni sous MSVC).
- Fichier produit : `message-vocal.ogg` (`audio/ogg`, Opus), que le serveur accepte (`application/ogg`) et que lisent Chrome, Firefox, Safari 17 et la WebView
  d'Android ; envoyé par `Client::envoyer_vocal` (`kind=voice`, `duration_ms`, `waveform` avant le fichier) puis en message sans texte par la file d'envoi, en
  réponse au message cité s'il y en a un. Formats de test versionnés dans `apps/estompe/crates/client/src/vocal/essais/` : `chrome.webm` (WebM à taille
  inconnue), `firefox.ogg`, `safari.m4a` (MP4 fragmenté, AAC), produits par ffmpeg comme les produirait MediaRecorder, et `natif.ogg`, produit par l'app ;
  les tests décodent les uns et font relire à ffmpeg (autre décodeur, celui de Chromium) ceux de l'app.
- Son : traits `Micro` et `SortieSon` de la crate plateforme (`SonSysteme`, `cpal` : WASAPI sous Windows, ALSA sous Linux), appareils par défaut du système, flux
  ouvert sur un fil dédié et fermé quand son `FluxSon` disparaît. Un refus d'accès (`PermissionDenied`) donne « Accès au micro refusé… », l'absence de micro
  « Aucun micro disponible… », toute autre erreur « Le micro est inutilisable. », sous le champ. Le choix du micro arrive avec les réglages du vocal (lot 4).
- Enregistrement (`ui/messagerie/saisie.slint`, global `DonneesVocal`) : le bouton « Envoyer » devient « Message vocal » quand le champ est vide, sans pièce ni
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
  lancé ; pas de geste « maintenir pour enregistrer » (tactile) ; pas d'annulation d'écho ni de réduction de bruit à l'enregistrement (le navigateur les fait,
  `nnnoiseless` arrive avec le vocal au lot 4) ; pas de transcription (lot 9).
- À vérifier sur une vraie machine (Windows et Linux) : micro et haut-parleurs réels, refus du micro par les réglages de confidentialité de Windows, message
  enregistré lu par le front web et l'inverse.

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

- L'app ne notifie que sur l'évènement temps réel `notification` (`Changement::Notification` du store), jamais sur `message.created` : sourdine
  d'un salon ou d'un serveur, « ne pas déranger », mentions et types de notification sont décidés par le serveur (`blocks/notifications`).
- `decider` : rien pour l'activité vocale de mon propre salon ; rien si la conversation de la notification est affichée **et** la fenêtre active
  (focus de winit) ; sinon une notification du système si la préférence « Notifications du bureau » est active.
- Regroupement : l'étiquette (`tag` du serveur) remplace une notification de même étiquette (toast de même `Tag`, `replaces_id` sous Linux).
- Son : celui du système avec la notification (audio par défaut du toast, indice `sound-name=message-new-instant` sous Linux), muet si le serveur la dit
  silencieuse. Écart assumé : le front joue un bip même quand les notifications du bureau sont coupées ; l'app native n'a pas de sortie audio avant le lot vocal.
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
  Une mise à jour imposée bloque l'app (écran « Mise à jour requise » de l'entrée), ou après 120 s si l'utilisateur est occupé (zone de saisie non
  vide, ou en appel). En appel, l'installation attend la fin de l'appel ou une confirmation (aucun vocal avant le lot 4 : jamais en appel aujourd'hui).
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

## Portabilité

Aucun `cfg(target_os)`, `cfg(windows)` ni `cfg(unix)` hors de `apps/estompe/crates/plateforme` : `scripts/estompe/natif-portabilite.sh` le vérifie
(seul `windows_subsystem = "windows"` est admis, pour ne pas ouvrir de console sous Windows).

La crate plateforme expose un trait par fonction : coffre de secrets, notifications, pastille de la barre des tâches, zone de notification,
installation d'une mise à jour, raccourcis globaux, ouverture d'une URL, capture d'écran et de fenêtre, capture du son, overlay, application au
premier plan, démarrage avec la session. Chaque trait a une méthode `disponible()`, que
l'interface consulte pour masquer ce qui n'existe pas, et l'implémentation `Indisponible`, qui répond `Erreur::Indisponible` sans paniquer.
`Plateforme::de_l_os()` assemble ce que l'OS sait faire. Sont implémentés sur les deux OS : le coffre de secrets (`keyring` : Credential Manager
sous Windows, Secret Service sous Linux) l'ouverture d'une URL `http` ou `https` dans le navigateur, le choix d'un dossier ou de fichiers et l'ouverture d'un fichier (`Fichiers` : `rfd` pour le
sélecteur, `explorer` ou `xdg-open` pour l'ouverture), la copie d'une image et la lecture d'une image ou d'une liste de fichiers (`PressePapiers`, `arboard`), le micro et la sortie du son (`Micro`, `SortieSon`, `cpal`), le lancement d'un processus d'arrière-plan
(`commande_d_arriere_plan`, utilisé pour ffmpeg), la zone de notification (`ZoneNotification`, `tray-icon`), les notifications (`Notifications` : toasts sous Windows, `notify-rust` sous Linux) et
l'instance unique (fonctions de `instance.rs`, `interprocess`). Sous Windows seulement : le démarrage avec la session, la pastille de la barre des tâches et l'installation d'une mise à jour (`InstallationSysteme`). Le reste arrive avec son lot, Windows d'abord.

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
- Préférences : thème, densité, réduction des animations, notifications, aperçus, journal de débogage, ordre des serveurs, géométrie de la
  fenêtre (retenue à la demande de fermeture et en quittant, tant que la fenêtre est visible et pas réduite : une fois masquée, sa position n'est plus connue ; une position hors de ±16 384 px, comme celle d'une fenêtre réduite sous Windows, n'est pas réappliquée), adresse du serveur, dernière vue (compte et adresse `#/s/…` ou `#/dm/…`), panneau replié, marge ouverte, smileys récents (`emoji_recents`), dernières recherches (`recherches_recentes`), messages vocaux écoutés (`vocaux_ecoutes`, 500 au plus). Écriture atomique ; un fichier illisible est mis de côté (`preferences.json.corrompu`) et recréé avec les valeurs par défaut.
- Adresse du serveur : `https://chat.plbls.fr` par défaut. Pour un serveur de test, la variable `ESTOMPE_API_URL`, sinon la préférence
  `adresse_serveur` ; une adresse invalide est écartée et notée dans le journal.
- Journal de débogage : écrit seulement si la préférence `journal_debogage` est activée, tournant à 512 Ko, exportable. Jetons, `Bearer` et
  paramètres secrets y sont masqués avant écriture.

## Types de l'API

`apps/estompe/crates/client/src/genere/types.rs` est généré depuis `docs/estompe/openapi.yaml` par `scripts/estompe/gen-types-client.mjs`.
Après un changement du contrat : `just estompe-natif-generer`. La CI échoue si le fichier versionné ne correspond plus (`--check`).

## Captures de rendu

`apps/estompe/natif/tests/captures.rs` rend chaque écran hors écran avec le moteur logiciel de Slint (backend de test, rendu en Rust pur,
identique d'une machine à l'autre), à 1180 × 900, avec des données fixes, le temps figé et les animations coupées, puis le compare à `tests/references/<écran>.png`. Un pixel
compte s'il s'écarte de plus de 24 sur un canal ; le test échoue au-delà de 0,1 % de pixels différents. En cas d'échec, la capture obtenue, la
référence et l'image des différences sont dans `target/tmp/captures/`, publiées par la CI dans l'artefact `estompe-natif-captures`.

- Ajouter un écran : une entrée dans `ECRANS` (nom, thème, fonction qui prépare la fenêtre), puis `just estompe-natif-captures` et versionner l'image.
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
chaque état. `coquille.rs` branche la coquille sur une fausse fabrique (store réel rempli par `Store::installer`, `FluxTempsReel::factice`) :
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
    `estompe-natif.exe`, `estompe.ico` (icône des raccourcis et `IconUri` des toasts), `desinstaller.exe` ;
  - raccourci du menu Démarrer, et sur le bureau (case cochée par défaut, sautée en mode silencieux) ;
  - registre (`HKCU`) : `Software\EstompeNatif` (dossier), désinstallation `Software\Microsoft\Windows\CurrentVersion\Uninstall\EstompeNatif`,
    identité des toasts `Software\Classes\AppUserModelId\fr.plbls.estompe.natif` (`DisplayName`, `IconUri`, mêmes valeurs que l'app).
    L'app Tauri garde son dossier, sa clé de désinstallation et l'identifiant `fr.plbls.estompe` : les deux cohabitent.
  - mode silencieux de mise à jour, `/S /RELANCER` : attend que `estompe-natif.exe` soit libre (l'app a quitté ; 120 × 500 ms au plus, en
    interactif un message « Fermez Estompe natif » avec Réessayer), installe, puis relance l'app si `/RELANCER` est passé ; en cas d'échec, relance l'exe en place.
  - désinstallation (refusée tant que l'app tourne) : programme, icône, raccourcis, clés ci-dessus, valeur `EstompeNatif` de `Run` (démarrage
    avec la session) et cache `%LOCALAPPDATA%\fr.plbls.estompe.natif\cache` (mises à jour téléchargées comprises) ; garde les préférences et
    brouillons (`%APPDATA%\fr.plbls.estompe.natif`), les journaux et le coffre de secrets.
  - non signé Authenticode pendant la bêta : SmartScreen avertit à la première installation.
- Contrôle (`scripts/estompe/natif-installeur-check.sh <installeur> <exe>`) : taille affichée et plafonnée (`TAILLE_MAX_MO`, 45 Mo), contenu
  listé par `7z` (exe, icône, désinstalleur), exe sans référence à `WebView2Loader` ni `EmbeddedBrowserWebView`.
- Manifeste de mise à jour `natif.json` (`scripts/estompe/natif-manifeste.mjs`), servi par la release fixe `estompe-natif-beta` :
  `https://github.com/Plblsfr/plbls.fr/releases/download/estompe-natif-beta/natif.json`. Format de `latest.json` de Tauri plus la taille :
  `{ version, notes, pub_date, mandatory, force, minVersion, message, platforms: { "windows-x86_64": { url, signature, taille } } }` ;
  `url` vise l'installeur de la release versionnée (`…/releases/download/estompe-natif-vX.Y.Z/Estompe-natif-beta-X.Y.Z-setup.exe`, redirigé
  vers `objects.githubusercontent.com` ou `release-assets.githubusercontent.com`), `signature` est le fichier `.minisig` en base64 (clé
  `apps/estompe/crates/client/src/mise_a_jour/cle.pub`, commentaire de confiance `estompe-natif X.Y.Z`), `taille` en octets.
- Essai local de l'installeur sous Linux : `apt-get install nsis p7zip-full`, puis dans `apps/estompe/natif/installeur/`
  `makensis -DVERSION=0.1.0 -DEXE=<un exe quelconque> estompe-natif.nsi` et le contrôle sur le résultat.

## CI

Jobs `estompe-natif-linux` et `estompe-natif-windows` de `ci.yml` : voir [ci.md](ci.md).
