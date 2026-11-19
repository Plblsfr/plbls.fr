# App native (Slint, sans webview)

« Estompe natif (beta) » est l'app de bureau écrite entièrement en Rust, sans webview, à côté de l'app Tauri qui reste la version stable.
Elle s'installe et tourne en même temps qu'elle. Ce qui lui manque encore est dans la [matrice de parité](natif-parite.md).
En place : la fenêtre, l'écran « À propos », la page de démonstration des composants, les écrans d'entrée (connexion, accès refusé, activation, invitation, mise à jour forcée) et la coquille (rail des serveurs, salons, messages privés, accueil, navigation) ; le contenu des salons arrive avec les lots suivants.

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
- `ui/composants/` : bouton, champ de saisie, case, interrupteur, modal, infobulle, icône, avatar, indicateur de chargement, zone défilante,
  compteur (pastille de non-lus ou de mentions), étiquette (« bientôt »), élément de liste, état vide, coins arrondis d'une image.
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
| Marge | `marge.slint` | `EmplacementMarge`, vide : point d'insertion de la marge « Membres » |

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
- Salons vocaux, cinéma, vidéos et musique sont listés avec l'étiquette « bientôt » ; leur vue l'explique. Un salon texte ou un message privé montre son nom et l'état vide « Les messages arrivent avec le lot 2 ».

### Brancher la marge de droite

L'emplacement est prêt ; le panneau des membres s'y ajoute sans toucher à la mise en page :

1. Interface : un composant (par exemple `ui/coquille/membres.slint`) placé comme enfant de `Coquille` dans `ui/app.slint` (`Coquille { MargeMembres { } }`) :
   il est rendu dans `EmplacementMarge`, à droite du contenu, sur `DonneesCoquille.largeur-marge` (320 px par défaut).
2. Passer `DonneesCoquille.marge-disponible` à vrai : le bouton « Membres » apparaît dans l'en-tête du contenu et Ctrl+. bascule `marge-ouverte`
   (rappel `marge-bascule`, branché dans `cycle.rs`).
3. Données : un modèle de plus dans `DonneesCoquille`, sa projection pure dans `projection.rs` (depuis `Etat::membres(serveur)` et `presence_de`),
   et sa cible dans `modeles.rs` : `Changement::MembreAjoute`, `MembreRetire`, `Membre` et `Presence`, aujourd'hui ignorés par `cibles`.
4. Une capture (clair et sombre) avec la marge ouverte.

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
| Cache | `%LOCALAPPDATA%\fr.plbls.estompe.natif\cache` | `$XDG_CACHE_HOME/fr.plbls.estompe.natif/cache` (défaut `~/.cache`) |
| Journaux (`estompe-debug.log`, `estompe-debug.1.log`) | `%LOCALAPPDATA%\fr.plbls.estompe.natif\journaux` | `$XDG_DATA_HOME/fr.plbls.estompe.natif/journaux` |
| Coffre de secrets | service `fr.plbls.estompe.natif` | service `fr.plbls.estompe.natif` |

- `ESTOMPE_NATIF_DOSSIER=<dossier>` place tout sous `<dossier>/{donnees,cache,journaux}` : une seconde instance, ou un essai sans toucher au profil.
- Préférences : thème, densité, réduction des animations, notifications, aperçus, journal de débogage, ordre des serveurs, géométrie de la
  fenêtre, adresse du serveur, dernière vue (compte et adresse `#/s/…` ou `#/dm/…`), panneau replié. Écriture atomique ; un fichier illisible est mis de côté (`preferences.json.corrompu`) et recréé avec les valeurs par défaut.
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
  par le `border-radius` de son parent : les images arrondies (icônes des serveurs) sont recouvertes par `Coins`, dans les deux moteurs.

## Tests d'interaction

Backend de test de Slint, un par fichier de `tests/` : `clavier.rs` et `connexion_clavier.rs` sans boucle d'évènements (`init_no_event_loop`),
`connexion.rs` et `coquille.rs` avec la boucle (`init_integration_test_with_mock_time`, un seul `#[test]` par fichier) : un fil pilote clique par `ElementHandle` et attend
chaque état. `coquille.rs` branche la coquille sur une fausse fabrique (store réel rempli par `Store::installer`, `FluxTempsReel::factice`) :
un non-lu ou une mention reçus en temps réel apparaissent sur le rail sans action, puis navigation, préférences, bandeau du réseau, section et fin de session. `ElementHandle` exige les informations de débogage de l'interface, que `build.rs` n'ajoute qu'au profil de débogage.
`mise_a_jour.rs` vérifie l'entrée sans interface contre un faux serveur qui compte les requêtes.

## CI

Jobs `estompe-natif-linux` et `estompe-natif-windows` de `ci.yml` : voir [ci.md](ci.md).
