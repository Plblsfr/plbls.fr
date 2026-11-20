# App native (Slint, sans webview)

« Estompe natif (beta) » est l'app de bureau écrite entièrement en Rust, sans webview, à côté de l'app Tauri qui reste la version stable.
Elle s'installe et tourne en même temps qu'elle. Ce qui lui manque encore est dans la [matrice de parité](natif-parite.md).
En place : la fenêtre, l'écran « À propos », la page de démonstration des composants, les écrans d'entrée (connexion, accès refusé, activation, invitation, mise à jour forcée) la coquille (rail des serveurs, salons, messages privés, accueil, navigation), la marge « Membres » (liste des membres, présence, activité, carte de profil) et la messagerie en lecture (messages des salons texte et des conversations privées, temps réel, marquage comme lu) ; l'écriture arrive avec le lot 3.

## Architecture

| Dossier | Rôle |
|---|---|
| `apps/estompe/natif` | binaire `estompe-natif` : interface Slint (`ui/`), moteur de rendu Skia, branchement des préférences et du journal (`src/session.rs`), écrans d'entrée (`src/entree.rs`, `src/ecrans.rs`), coquille (`src/coquille/`) |
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

## Interface

- `ui/genere/couleurs.slint` et `ui/genere/icones.slint` sont générés par `scripts/estompe/natif-da.mjs` depuis
  `web/estompe/src/styles/v3-jetons.css` (thèmes « Papier » et « Fusain ») et `web/estompe/src/components/Icon.svelte`. Après un changement de
  l'un ou de l'autre : `just estompe-natif-generer`. La CI échoue si les fichiers versionnés ne correspondent plus (`--check`).
- `ui/jetons.slint` : typographie, rayons, espacements et durées de [la direction artistique](direction-artistique.md) ; les trois polices
  variables (`polices/`, licence OFL) sont embarquées dans le binaire.
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
dans le navigateur la page du serveur (`ESTOMPE_API_URL` ou `https://chat.plbls.fr`) : l'installeur natif n'existe pas encore, ni le
téléchargement dans l'app.

## Coquille

Une fois l'entrée à `Connecte`, la coquille remplace les écrans d'entrée : rail des serveurs, panneau de navigation, zone de contenu et,
à droite, l'emplacement de la marge (vide pour l'instant).

| Élément | Interface (`ui/coquille/`) | Contenu |
|---|---|---|
| Rail | `rail.slint` | accueil (non-lus et mentions des messages privés), serveurs dans l'ordre choisi (image du serveur, sinon initiales ; trait de non-lus, pastille de mentions), « Rejoindre un serveur », entrées des sections ouvertes |
| Panneau | `panneau.slint` | salons du serveur courant (texte, esquisses, vocaux, liste simple comme `SidePanel.svelte`), ou accueil et messages privés, ou section ; barre du compte avec « Se déconnecter » ; repliable |
| Contenu | `contenu.slint`, `accueil.slint`, `bandeau.slint` | en-tête (nom du salon, sujet) ; accueil (salut, à rattraper, vocaux en cours) ; état vide qui annonce le lot du contenu ; bandeau « Reconnexion en cours… » ou hors ligne |
| Marge | `marge.slint`, `membres.slint`, `carte-profil.slint` | `EmplacementMarge` ; sur un serveur, la marge « Membres » (voir [Membres](#membres)) et la carte de profil |

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
| `src/messagerie/pont.rs` (`PontMessagerie`) | applique les `Changement::Messages` aux modèles, cache borné des rendus (512, LRU), chargement des pages, avatars et badges des auteurs, marquage comme lu, saisie en cours |
| `ui/messagerie/vue.slint`, `element.slint`, `donnees.slint` | liste, élément de message, global `DonneesMessagerie` |

### Virtualisation

`ListView` de Slint 1.18 instancie les lignes visibles et accepte des hauteurs différentes, mais estime la position par la hauteur moyenne des
lignes instanciées (`update_visible_instances`, `i-slint-core/model/repeater.rs`) : la hauteur du contenu et la barre de défilement changent à
chaque pas, et des lignes ajoutées au-dessus de la première instance (cas d'une page plus ancienne quand on est près du haut) décalent la vue.
L'app virtualise donc elle-même, avec `virtualisation::fenetre_de_hauteurs` de la crate client :

- le modèle `DonneesMessagerie.lignes` ne contient que les lignes de la fenêtre (l'écran et 600 px au-dessus et au-dessous) ; deux espaces
  (`haut`, `bas`) remplacent les autres dans un `Flickable` ;
- chaque ligne signale sa hauteur (`init` et `changed height` → `ligne-mesuree`) ; avant la mesure, une estimation tirée du texte et de la largeur ;
  un changement de largeur oublie les mesures ;
- une ancre (ligne sous le haut de l'écran et décalage) est rétablie après chaque changement : ajout en tête, mesure d'une ligne au-dessus,
  retrait par la borne du store. Collée en bas, la liste suit les nouveaux messages ; en lecture de l'historique, un message reçu ne la déplace pas
  et le bouton « N nouveaux messages » apparaît ;
- les pages plus anciennes ou plus récentes sont demandées à moins de 1 500 px d'un bord ; une ligne « Chargement des messages… » ou
  « Début de la conversation » occupe le bord ;
- à l'ouverture, la vue se place sur le premier non-lu (nombre de non-lus lu avant que le store ne les efface) ; sans non-lu, en bas ;
- les rappels de mesure et de défilement planifient un rafraîchissement au tour suivant (`slint::Timer` de durée nulle) : un rappel `init` qui
  modifierait le modèle pendant l'instanciation de ses lignes ferait paniquer le répéteur.

Mises à jour fines : `synchroniser` compare la fenêtre à son ombre ; une modification, un épinglage ou une réaction ne remplacent que la ligne du message,
une suppression la retire. La conversation entière est reprojetée côté Rust (au plus 600 messages), jamais le modèle Slint.

### Rendu du markdown, sélection et copie

Un paragraphe est un `StyledText` (Slint 1.18) : gras, italique, barré, code en ligne, liens. Le balisage est produit depuis le `Rendu` de la crate
client, texte échappé et délimiteurs entourés d'une espace sans chasse pour que les bords d'un style (espace, ponctuation) ne le cassent pas.
Titres, blocs de code, citations (barres) et listes (puces, retraits) sont des éléments Slint autour. Les mentions de personne sont des liens
(`m:` → carte de profil), celles de rôle ou de groupe sont colorées ; un passage masqué est un pavé cliquable (`s:` → révélé pour ce message) ;
un lien (`u:`) n'est ouvert par `OuvertureUrl` que si `lien_ouvrable` l'accepte, au clic comme à l'analyse.

Sélection : ni `Text` ni `StyledText` ne permettent de sélectionner du texte en 1.18 (seul `TextInput`, d'un seul style, le fait). La copie se fait
donc par message entier : bouton « Copier le message » au survol, ou Ctrl+C sur le message qui a le focus ; le texte copié est `texte_brut`, passé
au presse-papiers par un `TextInput` caché (`select-all()`, `copy()`).

Écarts : la police de texte embarquée n'a pas d'italique (l'italique n'est pas distinguable) ; le code en ligne utilise la police à chasse fixe générique
du système ; un lien est toujours souligné ; les emojis personnalisés restent `:nom:`.

### Élément de message et emplacements à venir

`CorpsMessage` (`ui/messagerie/element.slint`) : gouttière (avatar 36 px, ou heure au survol pour une suite groupée), réponse citée (clic :
`sauter_a`, chargement autour du message si besoin, puis mise en évidence 1,5 s), en-tête (pseudo à la couleur du rôle, « Bot », deux badges,
épingle, heure), corps (`TexteRiche`, 15 px), annexes, état d'envoi.

- Annexes : la structure `AnnexesMessage` (`pieces-jointes`, `sondage`, `reactions: [ReactionMessage]`, `lien`, `resume`) est remplie par
  `annexes()` (`lignes.rs`, `DonneesAnnexes`) et convertie par `annexes_slint()` (`pont.rs`). `AnnexesMessageVue` n'affiche pour l'instant que
  `resume` (« 2 pièces jointes · 👍 3 ») ; pièces jointes, sondages, réactions et aperçus de liens y remplaceront cette ligne.
- État d'envoi : champ `etat-envoi` de `LigneMessage`, vide en lecture ; l'écriture (lot 3) le remplira.
- Accusés de lecture (avatars « Vu par ») : pas encore affichés (`Store::lecteurs` est prêt).

### Temps réel et lecture

- Nouveau message, modification, suppression, épinglage, saisie (« carol est en train d'écrire… », expirée par la boucle de la session) : appliqués par `PontMessagerie::appliquer`.
- Lu : `signaler_lecture(en_bas, ancre)` quand le bas de la liste est atteint ou quitté, `definir_focus` quand la fenêtre gagne ou perd le focus.
  Le focus vient de l'évènement `Focused` de winit (`on_winit_window_event`, feature `unstable-winit-030` de Slint), sans code propre à un OS ;
  `Coquille::fenetre_active` est le même chemin, utilisé par les tests. Une lecture sur un autre appareil (`read.updated` à mon nom) retire la pastille.
- Le compte « Estompe » (notes de version) ouvre une conversation en lecture seule.

## Portabilité

Aucun `cfg(target_os)`, `cfg(windows)` ni `cfg(unix)` hors de `apps/estompe/crates/plateforme` : `scripts/estompe/natif-portabilite.sh` le vérifie
(seul `windows_subsystem = "windows"` est admis, pour ne pas ouvrir de console sous Windows).

La crate plateforme expose un trait par fonction : coffre de secrets, notifications, raccourcis globaux, ouverture d'une URL, capture d'écran
et de fenêtre, capture du son, overlay, application au premier plan, démarrage avec la session. Chaque trait a une méthode `disponible()`, que
l'interface consulte pour masquer ce qui n'existe pas, et l'implémentation `Indisponible`, qui répond `Erreur::Indisponible` sans paniquer.
`Plateforme::de_l_os()` assemble ce que l'OS sait faire. Sont implémentés sur les deux OS : le coffre de secrets (`keyring` : Credential Manager
sous Windows, Secret Service sous Linux) et l'ouverture d'une URL `http` ou `https` dans le navigateur. Le reste arrive avec son lot, Windows d'abord.

Les tests du coffre du système sont ignorés (et le disent) sur une machine sans coffre ; `ESTOMPE_COFFRE_EXIGE=1` les rend obligatoires, comme en CI.

## Données locales

Identifiant `fr.plbls.estompe.natif`, distinct de `fr.plbls.estompe` et `fr.plbls.estompe.test` de l'app Tauri : aucun fichier n'est partagé.

| | Windows | Linux |
|---|---|---|
| Données (`preferences.json`) | `%APPDATA%\fr.plbls.estompe.natif` | `$XDG_DATA_HOME/fr.plbls.estompe.natif` (défaut `~/.local/share`) |
| Cache (avatars dans `avatars/`) | `%LOCALAPPDATA%\fr.plbls.estompe.natif\cache` | `$XDG_CACHE_HOME/fr.plbls.estompe.natif/cache` (défaut `~/.cache`) |
| Journaux (`estompe-debug.log`, `estompe-debug.1.log`) | `%LOCALAPPDATA%\fr.plbls.estompe.natif\journaux` | `$XDG_DATA_HOME/fr.plbls.estompe.natif/journaux` |
| Coffre de secrets | service `fr.plbls.estompe.natif` | service `fr.plbls.estompe.natif` |

- `ESTOMPE_NATIF_DOSSIER=<dossier>` place tout sous `<dossier>/{donnees,cache,journaux}` : une seconde instance, ou un essai sans toucher au profil.
- Préférences : thème, densité, réduction des animations, notifications, aperçus, journal de débogage, ordre des serveurs, géométrie de la
  fenêtre, adresse du serveur, dernière vue (compte et adresse `#/s/…` ou `#/dm/…`), panneau replié, marge ouverte. Écriture atomique ; un fichier illisible est mis de côté (`preferences.json.corrompu`) et recréé avec les valeurs par défaut.
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
`messagerie.rs` (même backend, faux serveur HTTP de 300 messages, `ServeurMessages` de `tests/commun/`) : ouverture sur le premier non-lu, saisie, message reçu en lisant l'historique sans défilement, modification, épinglage et suppression par une seule opération sur le modèle, liens refusés, saut vers un message cité non chargé, non-lu fenêtre inactive puis lu au retour, lecture sur un autre appareil.
`messagerie_volume.rs` : salon de 50 000 messages remonté pendant 2 minutes simulées (lignes instanciées, modèles et mémoire bornés) ; un message repère garde sa position à l'écran quand une page plus ancienne arrive.
`mise_a_jour.rs` vérifie l'entrée sans interface contre un faux serveur qui compte les requêtes.

## CI

Jobs `estompe-natif-linux` et `estompe-natif-windows` de `ci.yml` : voir [ci.md](ci.md).
