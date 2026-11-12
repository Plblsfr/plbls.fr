# Éditeur de clips (app Windows)

Depuis la galerie de l'instant replay ([`instant-replay.md`](instant-replay.md)), « Modifier » ouvre un clip dans l'éditeur (issues #159 et #160) : assembler
plusieurs clips, garder des extraits, retirer des passages, changer la vitesse d'un passage, recadrer l'image, poser du texte, régler le son de chaque piste,
puis exporter un **nouveau** fichier MP4. Tout est local, aucun changement côté serveur. L'éditeur fait partie de la feature cargo `instant-replay` et
n'existe que dans l'app Windows.

Ce qui dépend d'un appareil Windows (webview, encodeurs matériels, jeu en cours) est **non vérifié** : voir « Vérification ».

## Fonctionnement

- **Projet** : un montage est un projet, un fichier `.estompe/projets/<id>.json` dans le dossier des clips. « Modifier » sur un clip rouvre le projet le plus
  récent qui ne contient que ce clip, ou en commence un (écrit à la première modification). La galerie liste les projets (« Montages ») : rouvrir, renommer,
  supprimer (confirmation ; les clips et les fichiers exportés restent). Chaque modification enregistre le projet : une écriture à la fois, la suivante part
  quand la précédente est finie, avec le dernier état. L'historique d'annulation n'est pas enregistré.
- **Ouverture** : `montage_ouvrir` prépare chaque clip du projet dans `.estompe/` : une piste audio par fichier (`<clip>.mp4.piste<N>.m4a`, copie sans
  réencodage) et, si le clip n'est pas en H.264, un aperçu H.264 sans son, en 720p au plus, à la cadence du clip (`<clip>.mp4.apercu.mp4`, progression par
  `montage://preparation`). Ces fichiers sont réutilisés tant qu'ils sont plus récents que le clip, comptent dans le quota, suivent le clip quand il est
  renommé et sont supprimés avec lui. Un clip H.264 est lu tel quel. `montage_clip` fait la même préparation pour un clip ajouté au montage ouvert.
- **Clip manquant** : un clip du projet disparu ou illisible est rendu dans `manquants` ; l'éditeur le signale et ouvre le reste. Le fichier du projet garde
  ses passages tant que le montage n'est pas modifié. Si tous les clips manquent, l'éditeur ne s'ouvre pas. La galerie signale le manque sur le projet.
  Renommer un clip dans la galerie met à jour les projets qui l'utilisent.
- **Plusieurs clips** : « Ajouter un clip » propose les clips de la galerie absents du montage ; le clip est ajouté à la fin. Les clips et les segments se
  réordonnent par glisser-déposer ou par leurs boutons « vers le début », « vers la fin ». Retirer un clip du montage retire ses segments, pas son fichier.
  20 clips et 30 minutes de montage final au plus : au-delà, l'ajout (ou le ralenti) est refusé avec un message, et le moteur refuse aussi.
- **Ligne de temps** : les segments se suivent dans l'ordre du montage, chacun à la largeur de sa durée dans le clip. Un point de coupe partage le segment
  sous la tête de lecture ; un segment se retire ou se restaure. Les deux poignées rognent le **bloc** du segment choisi (suite de segments du même clip qui
  se touchent) : rogner au-delà d'un point de coupe retire les segments dépassés, élargir depuis un segment retiré ajoute un segment gardé. Tous les temps
  sont calés sur les images du clip concerné. Les volumes (0 à 200 %, muet) se règlent piste par piste pour le clip du segment choisi.
- **Vitesse** : 0,25×, 0,5×, 1×, 1,5×, 2× ou 4× par segment ; la durée finale en tient compte. Le son suit la vitesse en gardant sa hauteur, ou « Couper le
  son du segment ». Un ralenti qui laisse moins d'images par seconde que la cadence de sortie (cadence du clip × vitesse) est signalé « Saccades ».
- **Recadrage** : un seul cadre pour tout le montage, déplaçable et redimensionnable dans l'aperçu (souris ou flèches, `Maj` pour la taille), rapports libre,
  16:9, 1:1, 9:16, 4:5. Le cadre est une fraction de la **toile** : l'image du premier clip du montage, où les autres clips sont inscrits sans déformation,
  avec des bandes noires.
- **Texte** : des blocs sur un intervalle du montage final (début, fin), plusieurs à la fois : contenu (200 caractères, plusieurs lignes), police, taille,
  couleur, contour ou fond, position par glisser dans l'aperçu ou aux flèches. Position et taille sont des fractions du cadre final (la taille, de son petit
  côté) : le texte suit le recadrage et reste dans le cadre tant qu'il y tient.
- **Aperçu** : composé dans la webview. Une vidéo muette par clip (`asset:`), seule celle du segment lu est affichée ; à la fin d'un segment la lecture passe
  au segment gardé suivant (`requestVideoFrameCallback`, `timeupdate` à défaut), à sa vitesse (`playbackRate`). Le son d'une piste est joué par Web Audio avec
  son gain : tampons décodés à 1×, calés sur la vidéo à 40 ms près ; à une autre vitesse, un élément audio par piste avec `preservesPitch`. La toile et le cadre
  sont posés par les mêmes calculs que l'export ; chaque bloc de texte est dessiné sur un canvas aux dimensions de sortie par la fonction qui produit l'image
  exportée.
- **Annuler, rétablir** : toutes les opérations (clips, ordre, coupes, vitesse, son, cadre, texte, réglages de sortie), 200 états au plus, tant que l'éditeur
  est ouvert.
- **Export** : « Exporter » demande la résolution et la cadence, puis `montage_exporter` lance ffmpeg dans un processus à part, en priorité basse
  (`BELOW_NORMAL`, comme les autres appels à ffmpeg), et rend la main. La progression arrive par `montage://export` ; on peut fermer l'éditeur, la galerie
  montre l'export (progression, « Annuler ») puis le nouveau clip. Le fichier est écrit dans `.estompe/export-<pid>-<id>.part`, puis renommé
  `<nom du projet> (montage).mp4` (` (montage 2)`… si le nom est pris). Son annexe porte `origine` (nom du premier clip) : la galerie affiche « Montage » et
  « d'après … ». Les clips ne sont ouverts qu'en lecture.
- **Sortie** : toujours réencodée, H.264 + AAC (160 kbit/s, stéréo, 48 kHz), cadence constante, `+faststart`, un seul format pour tout le montage. Résolution
  par défaut : celle du cadre dans la toile ; on peut choisir une hauteur plus petite (2160, 1440, 1080, 720, 480), jamais plus grande. Avec un rapport
  imposé, les dimensions sont le plus grand multiple pair exact du rapport qui tient dans le cadre (9:16 dans du 1080p : 594 × 1056). Cadence par défaut :
  celle du premier clip ; au choix jusqu'à la plus haute des clips. Débit vidéo : le plus élevé des clips (× 1,5 pour un clip HEVC), ramené à la surface de
  sortie, entre 2 et 50 Mbit/s. Aucune piste audible : le fichier n'a pas de piste audio.
- **Encodeur** : les encodeurs matériels du ffmpeg embarqué sont essayés dans l'ordre (`h264_nvenc`, `h264_amf`, `h264_qsv`, `h264_mf`), puis l'encodeur
  logiciel (`libopenh264` ; le build LGPL embarqué n'a pas `libx264`). Un encodeur qui échoue, même en cours d'export, fait reprendre l'export avec le suivant ;
  un export terminé par l'encodeur logiciel est signalé (`logiciel: true` et message).
- **Erreurs** : fichier disparu ou illisible à l'ouverture (message, la galerie est rechargée) ; clip du montage introuvable, quota déjà atteint ou place
  insuffisante avant l'export (refus) ; disque plein ou échec pendant l'export (message, fichiers partiels supprimés) ; quota franchi par le fichier exporté
  (il est gardé, message) ; ligne de commande trop longue pour Windows (30 000 caractères : plusieurs dizaines de passages, refus avec un message). Dans tous
  les cas le projet reste enregistré. Les fichiers `export-*.part` laissés par un arrêt brutal sont purgés par `replay_configurer` quand aucun export n'est en
  cours.

## Graphe de l'export

Un `filter_complex` unique, construit par `planifier` puis `graphe` (`montage.rs`) :

1. Chaque clip utilisé est une entrée ffmpeg. Un passage qui revient en arrière dans un clip déjà lu (segments réordonnés) prend une entrée de plus sur le
   même fichier : une entrée n'est jamais lue que vers l'avant, sans mettre d'images en attente.
2. Par passage gardé : `fps` à la cadence du clip, `trim` en numéros d'images, `setpts=(PTS-STARTPTS)/vitesse`, `fps` à la cadence de sortie, `tpad` puis
   `trim=end_frame` pour un nombre d'images exact, mise à l'échelle dans la toile (`scale`, `pad`), `crop` du cadre, `scale` à la sortie, `setsar=1`.
   Le nombre d'images de chaque passage vient du cumul des durées, arrondi une seule fois : le total est la durée affichée à une demi-image près.
3. Son par passage : volumes et `amix` des pistes non muettes du clip, `atrim`, `atempo` (0,5 puis 0,5 pour 0,25× ; 2 puis 2 pour 4×), 48 kHz stéréo,
   `apad` et `atrim` à la durée exacte du passage ; un passage muet ou sans piste reçoit du silence (`aevalsrc`) si le montage a du son ailleurs.
4. `concat` des passages, puis un `overlay` par bloc de texte, actif de la première à la dernière image de son intervalle (`enable='between(n,a,b)'`).

**Texte : une image, pas `drawtext`.** Le ffmpeg épinglé contient `drawtext` (libfreetype, libharfbuzz), mais chaque bloc est rendu par la webview sur un canvas
aux dimensions de sortie, envoyé en PNG avec `montage_exporter`, écrit dans `.estompe/export-<pid>-<id>-t<n>.part` puis incrusté par `overlay` : l'export porte
les mêmes glyphes que l'aperçu (accents, émojis de la police du système), sans fichier de police ni échappement de texte à passer à ffmpeg. Le moteur vérifie
qu'il reçoit une image PNG par bloc, aux dimensions exactes de la sortie.

**Polices** : Bricolage Grotesque, Instrument Sans, JetBrains Mono, celles de l'interface (`@fontsource-variable`, licence SIL OFL 1.1 : l'usage dans une image
ou une vidéo n'est pas restreint). Elles sont déjà dans le bundle du front : rien n'est ajouté à l'installeur.

**Géométrie partagée** : `toile`, `inscrire`, `cadre_pixels` / `cadrePixels`, `dimensions_sortie` / `dimensionsSortie` existent dans `montage.ts` et dans
`montage.rs`, avec les mêmes cas de test. Le front envoie des fractions (cadre, position et taille du texte), le moteur les revalide et les convertit en pixels
pairs. La géométrie du texte (`dessinerTexte`) n'existe que dans le front, puisque c'est lui qui produit l'image.

## Contrat avec le front

Commandes Tauri (objets en `camelCase`, erreurs en français). Sans la feature `instant-replay`, elles répondent « instant replay absent de ce build »
(`montage_exports` et `montage_projets` répondent une liste vide). `chemin` suit la règle de la galerie : un `.mp4` placé directement dans le dossier des clips.
`projet` est l'identifiant d'un projet (lettres, chiffres, tirets).

| Commande | Arguments | Réponse |
|---|---|---|
| `montage_ouvrir` | `chemin` (un clip) ou `projet` | `{ id, projet, clips: [clip], manquants: [nom de fichier] }` |
| `montage_clip` | `chemin` | un clip préparé : `{ chemin, nom, source, dureeS, largeur, hauteur, ips, codec, apercu, pistes: [{ nom, chemin }] }` |
| `montage_projets` | | `[{ id, nom, date, dureeS, clips, manquants }]`, du plus récent au plus ancien |
| `montage_renommer` | `projet`, `nom` | rien |
| `montage_supprimer` | `projet` | rien (le fichier du projet seulement) |
| `montage_enregistrer` | `projet`, `montage` | rien |
| `montage_exporter` | `projet`, `montage`, `images` (un PNG par bloc de texte, en tableau d'octets) | l'état initial de l'export (le suivi se fait par évènement) |
| `montage_annuler` | `id` | rien ; ffmpeg est tué, les fichiers partiels supprimés, puis `montage://export` passe à `annule` |
| `montage_exports` | | les exports en cours, dans le format de `montage://export` |

- `source` est le nom du fichier du clip (`<clip>.mp4`) ; `apercu` et `pistes[].chemin` sont des chemins lisibles par `asset:` ; `pistes[].nom` vaut « Jeu »,
  « Micro », puis « Piste N » (4 pistes au plus) ; `projet` (dans la réponse de `montage_ouvrir`) est le montage enregistré, ou `null` pour un projet neuf.
- Le montage reçu n'est jamais cru : version, 1 à 500 segments, sources qui sont des noms de fichiers `.mp4` sans chemin, 20 clips au plus, bornes finies,
  vitesse de la liste, 30 minutes au plus, volumes de 0 à 2 pour les seuls clips du montage, 50 blocs de texte valides au plus (contenu, police et habillage de
  la liste, couleur `#rrggbb`, fractions de 0 à 1), cadre dans la toile, rapport de la liste, sortie plausible ; à l'export, clips présents dans le dossier,
  bornes dans la durée de chaque clip (lue par ffmpeg), au moins un segment gardé. Une hauteur ou une cadence de sortie trop grande est ramenée au maximum.

| Évènement | Charge |
|---|---|
| `montage://preparation` | `{ nom, pct }` pendant la création de l'aperçu H.264 du clip `nom` |
| `montage://export` | `{ id, projet, nom, pct, etat, message, chemin, logiciel }` |

`etat` vaut `encours`, `fini`, `annule` ou `erreur`. `pct` vient de `out_time_us` (`-progress pipe:1` de ffmpeg) rapporté à la durée finale, vitesses comprises,
plafonné à 99 ; 100 n'est émis qu'avec `fini`, une fois le fichier renommé. Il repart de 0 quand l'export reprend avec un autre encodeur. `projet` est
l'identifiant du projet exporté, `nom` son nom, `chemin` celui du fichier exporté (avec `fini`), `message` le motif d'une erreur ou une remarque de fin
(encodeur logiciel, quota).

## Format du projet

```json
{
  "version": 2,
  "nom": "Soirée du 11",
  "segments": [
    { "source": "Jeu 2026-11-12 09-14-03.mp4", "debut": 0.5, "fin": 2, "garde": true, "vitesse": 1, "muet": false },
    { "source": "Jeu 2026-11-12 09-14-03.mp4", "debut": 2, "fin": 3, "garde": false, "vitesse": 1, "muet": false },
    { "source": "Autre 2026-11-11 21-02-40.mp4", "debut": 0, "fin": 4, "garde": true, "vitesse": 0.5, "muet": true }
  ],
  "volumes": { "Jeu 2026-11-12 09-14-03.mp4": [{ "volume": 0.5, "muet": false }, { "volume": 1, "muet": true }], "Autre 2026-11-11 21-02-40.mp4": [] },
  "textes": [{ "contenu": "Bien joué 🎉", "police": "Bricolage Grotesque", "taille": 0.08, "couleur": "#ffffff", "habillage": "contour", "x": 0.5, "y": 0.82, "debut": 2, "fin": 5 }],
  "cadre": { "x": 0.341796875, "y": 0, "l": 0.31640625, "h": 1, "rapport": "9:16" },
  "sortie": { "hauteur": null, "ips": null }
}
```

- `segments` : liste ordonnée ; chaque segment désigne son clip par son nom de fichier, ses bornes en secondes dans ce clip, s'il est gardé, sa vitesse et si
  son son est coupé. Le résultat est la suite des segments gardés. Un même passage peut figurer deux fois, dans n'importe quel ordre.
- `volumes` : par clip, une entrée par piste audio (`volume` de 0 à 2, `muet`).
- `textes` : `x`, `y` (centre du bloc) et `taille` (corps, par rapport au petit côté) sont des fractions du cadre final ; `debut` et `fin` sont des secondes du
  montage final ; `habillage` vaut `aucun`, `contour` ou `fond`.
- `cadre` : fractions de la toile (image du premier clip) ; `rapport` vaut `libre`, `16:9`, `1:1`, `9:16` ou `4:5`.
- `sortie` : hauteur et cadence choisies à l'export, `null` pour celles du cadre et du premier clip.
- **Version 1** (#159, `.estompe/<clip>.mp4.montage.json`) : à la lecture de la liste des projets ou à l'ouverture, chaque ancien fichier devient un projet
  version 2 (source ramenée au nom actuel du clip, vitesse 1, pas de texte, cadre entier, nom du clip) et l'ancien fichier est supprimé. Un fichier illisible
  est supprimé sans projet.
- À la lecture, le front recale les temps sur les images, ramène les réglages dans leurs bornes et écarte les segments d'un clip manquant ; un projet illisible
  ou d'une autre version est ignoré (montage neuf).
- Types : `Montage`, `Segment`, `Volume`, `Texte`, `Cadre`, `Sortie` dans `apps/estompe/windows/src/montage.ts` et dans `src-tauri/src/replay/montage.rs`.

## Code

| Fichier | Rôle |
|---|---|
| `src/montage.ts` | modèle et logique pure : rogner, couper, retirer, restaurer, vitesse, ordre des segments et des clips, limites, ligne de temps et temps du montage, enchaînement à la lecture, pas à pas, géométrie (toile, cadre, sortie), texte (réglages, dessin), historique, lecture du projet, formes d'onde, raccourcis |
| `src/montage.svelte.ts` | état de l'éditeur (`editeur`), ouverture, ajout d'un clip, enregistrement du projet, images des textes, export, liste des projets, suivi des évènements dans `replay.travaux` |
| `src/ReplayEditeur.svelte` | vue de l'éditeur dans l'espace « Clips » : clips du montage, aperçu (toile, cadre, textes), ligne de temps, pistes, panneaux du segment, du recadrage et du texte, raccourcis, export |
| `src/ReplayGalerie.svelte` | liste « Montages » (rouvrir, renommer, supprimer) au-dessus des clips |
| `src-tauri/src/replay/montage.rs` | validation, géométrie, plan à l'image près, arguments ffmpeg, progression, préparation, projets (lecture, reprise de la version 1, renommage), export, annulation, commandes |

Raccourcis (affichés sous la ligne de temps, sans effet depuis un champ de saisie) : `Espace` lecture ou pause, `←` `→` image précédente ou suivante,
`C` point de coupe, `Suppr` retirer ou restaurer le segment choisi, `Ctrl + Z` annuler, `Ctrl + Y` ou `Ctrl + Maj + Z` rétablir. Les poignées, la règle, le cadre,
les blocs de texte et leurs intervalles se règlent aussi au clavier (flèches, `Maj` pour un pas plus grand ou pour la taille du cadre).

## Vérification

« Test » : test unitaire lancé en CI (`montage.test.ts` par vitest ; `cargo test --lib --features instant-replay replay`, job `estompe-tauri`). « ffmpeg réel » : test Rust qui
fabrique des clips de synthèse (H.264 et HEVC, résolutions et cadences différentes, avec et sans son), les prépare et les exporte avec le ffmpeg de la machine, puis
relit le fichier image par image et échantillon par échantillon ; ignoré avec un message s'il manque. Ce ffmpeg n'est pas le binaire Windows embarqué, et
l'encodeur y est `libx264` ; les filtres employés (`fps`, `trim`, `setpts`, `tpad`, `scale`, `pad`, `crop`, `setsar`, `split`, `volume`, `amix`, `asplit`, `atrim`,
`asetpts`, `atempo`, `aformat`, `apad`, `aevalsrc`, `concat`, `overlay`) et le lecteur `png_pipe` ont été retrouvés dans le binaire embarqué. « Chromium » : vue
contrôlée dans le Chromium de Playwright sur le bundle Windows, commandes Tauri simulées, aperçus VP9. « Non vérifié » : jamais exécuté sur un appareil Windows.

Critères de #160 :

| Critère | État |
|---|---|
| C1 trois clips réordonnés, sans image noire ni désynchronisation aux raccords | ordre et plan : test ; export de trois clips dans l'ordre choisi, première et dernière image de chaque passage, son présent jusqu'au raccord et silence 40 ms après : ffmpeg réel ; ajout, réordonnancement (glisser et boutons), enchaînement à la lecture : Chromium. Sur des clips du moteur, à l'œil et à l'oreille : non vérifié |
| C2 1080p60 H.264 + 1440p144 HEVC, un seul fichier H.264 sans déformation | plan et graphe (mise à l'échelle, bandes) : test ; clips H.264 320 × 240 à 30 i/s et HEVC 640 × 360 à 60 i/s, sortie unique à la résolution et à la cadence choisies, bandes noires à la place attendue : ffmpeg réel. Aux résolutions et cadences du critère, sur appareil : non vérifié |
| C3 texte de 2 s à 5 s à une image près, position et taille de l'aperçu | intervalle en images : test ; image incrustée présente de la première à la dernière image de l'intervalle, absente juste avant et juste après, à sa place : ffmpeg réel (image de synthèse) ; image PNG produite par le canvas de l'aperçu, aux dimensions de sortie : Chromium. Image rendue par WebView2 puis relue dans le fichier exporté : non vérifié |
| C4 deux blocs simultanés, accents et émojis | deux images incrustées en même temps : ffmpeg réel ; accents et émoji dessinés, deux blocs affichés : Chromium (émoji de la police du système). Dans WebView2 : non vérifié |
| C5 recadrage 9:16 | géométrie : test (mêmes cas en TS et en Rust) ; fichier exporté en 9:16 exact, contenu du cadre et bandes à leur place, texte dans le cadre : ffmpeg réel ; cadre déplacé, redimensionné, aperçu limité au cadre : Chromium |
| C6 0,5× deux fois plus long, 2× deux fois moins, durée affichée | durée et nombre d'images : test ; nombre d'images exact de chaque passage et durée du fichier : ffmpeg réel ; durée affichée : Chromium |
| C7 aperçu conforme à l'export | mêmes fonctions de géométrie et même dessin du texte des deux côtés : test ; ordre, cadre, texte, vitesse de lecture : Chromium. Comparaison de l'aperçu WebView2 et du fichier exporté : non vérifié |
| C8 annuler et rétablir sur les nouvelles opérations | test (dix-huit opérations) ; Chromium (vingt-six opérations annulées puis rétablies, projet identique) |
| C9 projet rouvert à l'identique, clip manquant signalé | sérialisation, reprise de la version 1, clip manquant : test ; projet rouvert, clip supprimé signalé, reste ouvert : ffmpeg réel et Chromium. Après un vrai redémarrage de l'app : non vérifié |
| C10 fichiers d'origine inchangés | contenu des trois clips comparé avant et après export : ffmpeg réel |
| C11 jeu prioritaire pendant un export | priorité basse du processus ffmpeg ; non vérifié (un encodeur matériel partage la carte graphique avec le jeu) |
| C12 logique couverte par des tests en CI | test |

Les critères de #159 restent couverts par les mêmes tests (ouverture, découpe à l'image près, volumes par piste, annulation d'un export, progression, fichier
disparu, quota, disque plein).

Non vérifié par ailleurs : durée d'un export (aucun seuil fixé, à mesurer sur appareil) ; encodeurs matériels et `libopenh264` du binaire embarqué ; reprise en
logiciel après un échec matériel réel ; mémoire prise par le décodage des pistes (environ 115 Mo par piste stéréo de 5 minutes, pour chaque clip du montage) et
par une vidéo d'aperçu par clip ; en-têtes CORS du protocole `asset:` pour lire les pistes par `fetch` et par un élément audio relié à Web Audio (son de
l'aperçu à une vitesse autre que 1×) ; fluidité de l'aperçu au passage d'un clip à l'autre (la vidéo suivante est positionnée au moment du raccord) ; lecture
de `png_pipe` et `tpad` par le binaire embarqué en conditions réelles.
