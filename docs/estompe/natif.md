# App native (Slint, sans webview)

« Estompe natif (beta) » est l'app de bureau écrite entièrement en Rust, sans webview, à côté de l'app Tauri qui reste la version stable.
Elle s'installe et tourne en même temps qu'elle. Ce qui lui manque encore est dans la [matrice de parité](natif-parite.md).
En place : la fenêtre, l'écran « À propos », la page de démonstration des composants, les écrans d'entrée (connexion, accès refusé, activation, invitation, mise à jour forcée) la coquille (rail des serveurs, salons, messages privés, accueil, navigation), la marge « Membres » (liste des membres, présence, activité, carte de profil) et la messagerie en lecture (messages des salons texte et des conversations privées, emoji, pièces jointes, aperçus de liens, sondages, réactions, temps réel, marquage comme lu) et le début de l'écriture (zone de saisie, envoi, modification, brouillons, voir [Écriture](#écriture)), l'intégration au bureau (instance unique, zone de notification, voir [Intégration au bureau](#intégration-au-bureau) ; notifications du système et pastille de mentions, voir [Notifications](#notifications)) ; le reste de l'écriture arrive avec le lot 3.

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

Prérequis sous Linux (Debian, Ubuntu) : `libfontconfig1-dev libfreetype-dev libxkbcommon-x11-0` et un pilote OpenGL (Mesa). Sous Windows : la
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
  liste virtualisée (`ListeVirtuelle`), compteur (pastille de non-lus ou de mentions), étiquette (« bientôt »), badge de rôle (« Bot », « Créateur »), élément de liste, état vide, coins arrondis d'une image.
- Un composant interactif placé dans un modal reçoit `calque: 1` : tant qu'un modal est ouvert, ceux du dessous n'acceptent ni focus ni clic.
- L'écran « À propos » affiche la version et l'attribution exigée par la licence royalty-free de Slint (`AboutSlint`).

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
(message du serveur, version installée et minimale) : la machine n'émet plus aucune requête, seul le bouton de téléchargement agit. Il ouvre
dans le navigateur la page du serveur (`ESTOMPE_API_URL` ou `https://chat.plbls.fr`) ; le téléchargement et l'installation dans l'app
(installeur de la [Livraison](#livraison)) ne sont pas encore branchés.

## Coquille

Une fois l'entrée à `Connecte`, la coquille remplace les écrans d'entrée : rail des serveurs, panneau de navigation, zone de contenu et,
à droite, la marge.

| Élément | Interface (`ui/coquille/`) | Contenu |
|---|---|---|
| Rail | `rail.slint` | accueil (non-lus et mentions des messages privés), serveurs dans l'ordre choisi (image du serveur, sinon initiales ; trait de non-lus, pastille de mentions), « Rejoindre un serveur », entrées des sections ouvertes |
| Panneau | `panneau.slint` | salons du serveur courant (texte, esquisses, vocaux, liste simple comme `SidePanel.svelte`), ou accueil et messages privés, ou section ; barre du compte avec « Se déconnecter » ; repliable |
| Contenu | `contenu.slint`, `accueil.slint`, `bandeau.slint` | en-tête (nom du salon, sujet) ; accueil (salut, à rattraper, vocaux en cours) ; état vide qui annonce le lot du contenu ; bandeau « Reconnexion en cours… » ou hors ligne |
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
- Messages privés du panneau : triés par activité (`lignes_privees` du store, une conversation qui reçoit un message remonte), avec non-lus, mentions et présence par formes sur l'avatar.

### Membres

Sur un serveur, `DonneesCoquille.marge-disponible` passe à vrai : le bouton « Membres » de l'en-tête et Ctrl+. ouvrent ou ferment la marge, le
bouton « Fermer la marge » la ferme ; l'ouverture est retenue dans les préférences (`marge_ouverte`, ouverte par défaut comme le front). Hors d'un serveur, il n'y a pas de marge.

- Interface : `MargeMembres` (`ui/coquille/membres.slint`), enfant de `Coquille` dans `ui/app.slint`, instancié seulement quand la marge est ouverte.
  En tête, l'onglet « Membres », le compteur `MEMBRES · en ligne/total` et, au-delà de 8 membres, le filtre (pseudo et pseudo de serveur, sans casse ni accents).
- Liste : groupes des rôles affichés séparément (`hoist`, du plus haut au plus bas, comme `grouper_membres`), puis « EN LIGNE » et « HORS LIGNE » pour les autres ;
  dans chaque groupe, en ligne d'abord puis par pseudo. Une ligne : avatar avec pastille de présence, pseudo de serveur à la couleur du rôle (atténué hors ligne),
  cadenas d'exclusion, « Bot », « Créateur », deux badges puis « +N » (médaille à la couleur du badge), jeu en cours. Un clic ouvre la carte de profil.
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
| `src/messagerie/annexes.rs` | projections pures : carte d'aperçu d'un lien, taille d'une vignette avant chargement, chemin sûr d'un fichier enregistré |
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
| vidéo, replay, message vocal | puce « bientôt dans l'app native » (lot 9) |
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
  prochaine clôture visible). Voter, clore et supprimer arrivent au lot 3.

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
- Désépingler est une écriture : lot 3.

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

## Portabilité

Aucun `cfg(target_os)`, `cfg(windows)` ni `cfg(unix)` hors de `apps/estompe/crates/plateforme` : `scripts/estompe/natif-portabilite.sh` le vérifie
(seul `windows_subsystem = "windows"` est admis, pour ne pas ouvrir de console sous Windows).

La crate plateforme expose un trait par fonction : coffre de secrets, notifications, pastille de la barre des tâches, zone de notification,
installation d'une mise à jour, raccourcis globaux, ouverture d'une URL, capture d'écran et de fenêtre, capture du son, overlay, application au
premier plan, démarrage avec la session. Chaque trait a une méthode `disponible()`, que
l'interface consulte pour masquer ce qui n'existe pas, et l'implémentation `Indisponible`, qui répond `Erreur::Indisponible` sans paniquer.
`Plateforme::de_l_os()` assemble ce que l'OS sait faire. Sont implémentés sur les deux OS : le coffre de secrets (`keyring` : Credential Manager
sous Windows, Secret Service sous Linux) l'ouverture d'une URL `http` ou `https` dans le navigateur, le choix d'un dossier et l'ouverture d'un fichier (`Fichiers` : `rfd` pour le
sélecteur, `explorer` ou `xdg-open` pour l'ouverture) et la copie d'une image (`PressePapiers`, `arboard`), la zone de notification (`ZoneNotification`, `tray-icon`), les notifications (`Notifications` : toasts sous Windows, `notify-rust` sous Linux) et
l'instance unique (fonctions de `instance.rs`, `interprocess`). Sous Windows seulement : le démarrage avec la session et la pastille de la barre des tâches. Le reste arrive avec son lot, Windows d'abord.

Les tests du coffre du système sont ignorés (et le disent) sur une machine sans coffre ; `ESTOMPE_COFFRE_EXIGE=1` les rend obligatoires, comme en CI.
De même, les tests qui ont besoin du bus de session D-Bus (zone de notification, faux serveur de notifications sous Linux) sont ignorés sans bus, et obligatoires avec `ESTOMPE_DBUS_EXIGE=1` (job `estompe-natif-linux`).

## Données locales

Identifiant `fr.plbls.estompe.natif`, distinct de `fr.plbls.estompe` et `fr.plbls.estompe.test` de l'app Tauri : aucun fichier n'est partagé.

| | Windows | Linux |
|---|---|---|
| Données (`preferences.json`) | `%APPDATA%\fr.plbls.estompe.natif` | `$XDG_DATA_HOME/fr.plbls.estompe.natif` (défaut `~/.local/share`) |
| Cache (avatars dans `avatars/`, images dans `medias/`) | `%LOCALAPPDATA%\fr.plbls.estompe.natif\cache` | `$XDG_CACHE_HOME/fr.plbls.estompe.natif/cache` (défaut `~/.cache`) |
| Journaux (`estompe-debug.log`, `estompe-debug.1.log`) | `%LOCALAPPDATA%\fr.plbls.estompe.natif\journaux` | `$XDG_DATA_HOME/fr.plbls.estompe.natif/journaux` |
| Coffre de secrets | service `fr.plbls.estompe.natif` | service `fr.plbls.estompe.natif` |
| Instance unique | tube nommé `\\.\pipe\fr.plbls.estompe.natif-<empreinte>` | `$XDG_RUNTIME_DIR/fr.plbls.estompe.natif/instance.sock` (sinon `<données>/instance.sock`) |

- `ESTOMPE_NATIF_DOSSIER=<dossier>` place tout sous `<dossier>/{donnees,cache,journaux}` : une seconde instance, ou un essai sans toucher au profil.
- Préférences : thème, densité, réduction des animations, notifications, aperçus, journal de débogage, ordre des serveurs, géométrie de la
  fenêtre (retenue à la demande de fermeture et en quittant, tant que la fenêtre est visible et pas réduite : une fois masquée, sa position n'est plus connue ; une position hors de ±16 384 px, comme celle d'une fenêtre réduite sous Windows, n'est pas réappliquée), adresse du serveur, dernière vue (compte et adresse `#/s/…` ou `#/dm/…`), panneau replié, marge ouverte. Écriture atomique ; un fichier illisible est mis de côté (`preferences.json.corrompu`) et recréé avec les valeurs par défaut.
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
  en bas, flèche haut et Échap, modification, refus 403 puis « Réessayer », brouillon retrouvé en changeant de salon et relu depuis le disque.
`messagerie_volume.rs` : salon de 50 000 messages remonté pas à pas (lignes instanciées, modèles et mémoire bornés) ; un message repère garde sa position à l'écran quand une page plus ancienne arrive.
  Par défaut 8 pas de 1 400 px (8 s simulées, au moins 3 pages anciennes après le premier quart, environ 22 s en debug) ; `ESTOMPE_VOLUME=long` (ou `just estompe-natif-volume`) rejoue le parcours long de 120 pas de 1 400 px (2 minutes simulées, au moins 10 pages).
`mise_a_jour.rs` vérifie l'entrée sans interface contre un faux serveur qui compte les requêtes.
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
