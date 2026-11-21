# Mesures mémoire et CPU de l'app native

Point de contrôle de l'épopée [#271](https://github.com/Plblsfr/plbls.fr/issues/271), à la fin du lot 4 ([#331](https://github.com/Plblsfr/plbls.fr/issues/331)) :
sur le scénario « Partie de jeu avec overlays » de la mesure intégrée, 5 sessions avec l'app Tauri et 5 avec l'app native, la mémoire totale
de l'app native doit être au moins divisée par deux et son CPU au repos inférieur. Sinon l'épopée s'arrête et est réévaluée.

## Mesure intégrée

```sh
estompe-natif --mesures <fichier.json> [--scenario <id>] [--attente <s>] [--duree <s>]
```

- Dès le lancement, le fil `mesures` relève chaque seconde la mémoire privée, le working set, son pic et le temps CPU du processus
  (`Releveur` de `apps/estompe/crates/client/src/mesures.rs`, compteurs lus par `MesureProcessus` de la crate plateforme).
  L'app native n'a ni webview ni processus fils : un seul processus, de type et de rôle `hote`.
- Le scénario commence après `--attente` (0 par défaut, le temps de se connecter, de rejoindre le salon et de lancer le jeu) et dure `--duree`,
  par défaut celle du scénario du même nom de l'app Tauri (`jeu` et `repos` : 300 s ; scénario par défaut : `jeu`).
- À la fin du scénario, l'app se ferme et écrit l'export ; fermée avant, elle l'écrit aussi, avec un scénario écourté.
- L'export suit le format `estompe-mesures/1` de l'app Tauri (`apps/estompe/windows/src/mesures.ts`) : échantillons, marqueur du scénario et statistiques
  calculées de la même façon (médiane et pic de la mémoire, CPU en % d'un cœur). `app` vaut `natif <version>`, `systeme.webview2` est nul,
  `systeme.os` s'ajoute. Pas de sonde : intervalles entre images, latences et temps de démarrage sont absents (« — » dans la synthèse).
  Rien n'est envoyé ; l'export ne contient ni message ni identifiant.
- Compteurs : sous Windows `GetProcessMemoryInfo` (`PrivateUsage`, `WorkingSetSize`, `PeakWorkingSetSize`) et `GetProcessTimes`, comme l'app Tauri ;
  sous Linux `/proc/self/status` (mémoire privée = `RssAnon` + `VmSwap`, working set = `VmRSS`, pic = `VmHWM`) et `/proc/self/stat`.
  La mémoire privée de Linux ne compte que l'anonyme résidente, celle de Windows toute la mémoire privée engagée : ne comparer que des mesures du même OS.

La synthèse de l'app Tauri lit les deux formats ; la colonne de l'app native s'appelle `natif <version>` :

```sh
node apps/estompe/windows/scripts/synthese-mesures.mjs mesures/ --sortie synthese.md
```

`just estompe-natif-mesures app <dossier> [sessions] [scénario] [durée]` (`scripts/estompe/natif-mesures.sh`) lance l'app une fois par session
(variable `ATTENTE` pour `--attente`), puis la synthèse.

Tests : `mesures::tests` (crate client) produit l'export de référence `apps/estompe/crates/client/tests/donnees/mesures-natif.json`
(`ESTOMPE_MESURES=regenerer` le réécrit), que `apps/estompe/windows/src/mesures.test.ts` passe à `synthese-mesures.mjs` à côté d'un export de l'app Tauri ;
`mesures::tests` de l'app (options, marqueur, export écrit), `processus` de la crate plateforme (lecture de `/proc`, compteurs du processus courant).

## Protocole du point de contrôle

Sur une machine Windows réelle, à la main (C2 de #331) :

1. Même machine, sur secteur, résolution et mise à l'échelle fixées, rien d'autre d'ouvert que le jeu et l'app mesurée. Même jeu, même partie
   (même carte, ou un écran du jeu qui ne bouge pas), même compte, même salon vocal, avec les deux mêmes autres participants sur d'autres machines,
   dont un parle pendant la mesure. Durée : 5 minutes par session.
2. App Tauri : Paramètres › Débogage, « Démarrer une session », rejoindre le salon, lancer le jeu, scénario « Partie de jeu avec overlays », puis
   « Arrêter la session » et « Exporter les mesures ». 5 sessions overlays affichés, puis 5 sessions overlays masqués : l'app native n'a pas encore
   d'overlay (lot 7), elle se compare aux sessions sans overlays (point ouvert de #331).
3. App native : `estompe-natif.exe --mesures mesures\natif-jeu-1.json --attente 60` ; dans la minute, rejoindre le salon et lancer le jeu ;
   l'app se ferme d'elle-même après les 5 minutes du scénario. 5 sessions.
4. Synthèse des exports. Les deux chiffres de la décision : rapport de la mémoire privée totale médiane (app Tauri sans overlays sur app native),
   au moins 2 ; CPU total du scénario (l'app tourne derrière le jeu, au repos), plus bas pour l'app native. Le scénario « Repos 5 min » des deux apps
   (`--scenario repos`, sur un salon texte) peut s'y ajouter pour le CPU au repos hors jeu.
5. Exports et synthèse joints à l'épopée, décision écrite : continuer, corriger, ou arrêter.

## Mesures faites sous Linux

Indicatives seulement : elles ne remplacent pas le protocole ci-dessus. Machine de développement en conteneur : Intel Xeon à 2,10 GHz, 4 cœurs,
16 Go, Ubuntu 24.04, serveur X virtuel (Xvfb) sans GPU ; Skia dessine par OpenGL logiciel (Mesa llvmpipe), la surface wgpu n'étant pas disponible.
Build release (`cargo build --release`, SDK LiveKit compris, binaire de 56 987 272 octets).

### L'app au repos

Écran de connexion (sans compte, voir plus bas), 5 sessions du scénario `repos` de 2 minutes après 20 s d'attente
(`ATTENTE=20 just estompe-natif-mesures app <dossier> 5 repos 120`, profil vide) :

| Métrique (médiane des 5 sessions) | App native |
|---|---:|
| Mémoire privée, médiane (Mo) | 50,5 (50,4 à 50,7 selon la session) |
| Working set, médiane (Mo) | 145,3 (144,8 à 147,8) |
| Working set, pic de la session (Mo) | 145,3 |
| CPU pendant le repos (% d'un cœur) | 0 (30 à 50 ms de CPU en 2 minutes, moins de 0,05 %) |
| CPU du lancement, fenêtre affichée comprise (ms) | 210 à 290 |

Le working set compte les bibliothèques partagées chargées, dont le pilote OpenGL logiciel de Mesa ; sur une machine avec GPU, il sera différent.
Quand rien ne bouge à l'écran, le CPU au repos est nul à la précision du compteur (10 ms).

### Le moteur du vocal

Aucun serveur Estompe ni compte n'est joignable ici : l'app ne peut pas se connecter, donc pas rejoindre un salon. Le moteur du vocal est mesuré
à part par le banc `mesure_vocal` (exemple de la crate client) : la même `SessionVocale` que l'app (SDK LiveKit, APM de WebRTC, `nnnoiseless`,
détection de voix, mixage, limiteur, mode de bruit « renforcé » par défaut), sur un runtime tokio à un fil comme celui de l'app, contre le serveur LiveKit
de `deploy/docker-compose.dev.yml` (même version, lancé en local), sans interface. Les périphériques sont factices : une horloge de 10 ms pousse
dans le micro un bruit de fond à -60 dBFS (et la voix versionnée `voix-16k.wav` en boucle pendant la phase « parole ») et tire la sortie.
D'autres processus du même banc, des robots, rejoignent le salon et parlent sans arrêt.

```sh
just estompe-natif-mesures vocal <dossier> 0 1 4    # ESTOMPE_LIVEKIT_URL, PHASES=repos:30,silence:90,parole:90
```

Phases : `repos` (session créée, salon pas encore rejoint), `silence` (dans le salon, sans parler), `parole` (on parle).

| Autres participants (qui parlent) | Phase | Mémoire privée, médiane (Mo) | Working set, médiane (Mo) | CPU (% d'un cœur) |
|---|---|---:|---:|---:|
| 0 | repos | 2,2 | 8,9 | 0,4 |
| 0 | silence | 5,4 | 19,5 | 7,1 |
| 0 | parole | 5,5 | 19,6 | 8,4 |
| 1 | repos | 2,2 | 9,5 | 0,3 |
| 1 | silence | 6,2 | 20,2 | 8,8 |
| 1 | parole | 6,2 | 20,2 | 9,6 |
| 4 | repos | 2,2 | 9,2 | 0,3 |
| 4 | silence | 7,5 | 21,4 | 13,5 |
| 4 | parole | 7,6 | 21,5 | 13,3 |

Une session par configuration, 90 s par phase (`PHASES=repos:30,silence:90,parole:90`) ; avec 1 et 4 autres participants, leur voix a bien été reçue
et jouée (8 174 et 8 414 trames de 10 ms non silencieuses). Le SDK et ses fils restent légers en mémoire (moins de 8 Mo privés à 5 dans le salon,
10 fils). Le CPU vient surtout du traitement continu des trames de 10 ms : chaîne du micro (APM et `nnnoiseless`, 2,7 % d'un cœur mesurés seuls,
voir [natif.md](natif.md#chaîne-du-micro)), encodage et décodage Opus, mixage ; il croît avec le nombre de voix reçues. Les robots tournent
sur la même machine de 4 cœurs que le client mesuré.

## Point de contrôle : décision

**Non prise.** Les deux chiffres de la décision (rapport de la mémoire totale, CPU au repos) viennent des 10 sessions du protocole, sur une machine
Windows avec un jeu réel (C2 et C3 de #331), qui restent à faire : rien n'a été exécuté sous Windows ici, et l'app Tauri n'a pas de mesure de
référence dans le dépôt.

Ce que disent les mesures de Linux, sans valeur de décision :

- l'app native au repos tient en un processus d'environ 50 Mo privés (écran de connexion ; une coquille connectée avec ses salons, avatars et messages
  en ajoutera) et ne consomme pas de CPU quand rien ne bouge à l'écran ;
- le vocal ajoute de 3 à 6 Mo privés (une dizaine de Mo de working set) et 7 à 14 % d'un cœur de ce Xeon à 2,1 GHz selon le nombre de voix ; la part du CPU qui revient
  au rendu Skia pendant un appel (indicateur de parole, jauge du micro lue toutes les 50 ms) n'est pas mesurée ici ;
- à comparer, pour l'ordre de grandeur seulement, aux ≈ 64 Mo de la seule page d'overlay dans Chromium ([overlay-natif.md](overlay-natif.md)).

Point à surveiller pour la décision : le CPU du vocal, qui tourne même micro coupé (la chaîne mesure le niveau en continu) ; s'il dépassait celui de l'app
Tauri pendant la partie, le mode de bruit « standard » (sans `nnnoiseless`) ou l'arrêt de la chaîne micro coupé sont les premiers leviers.
