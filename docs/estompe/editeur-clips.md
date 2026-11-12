# Éditeur de clips (app Windows)

Depuis la galerie de l'instant replay ([`instant-replay.md`](instant-replay.md)), « Modifier » ouvre un clip dans un éditeur simple (issue #159) : garder un extrait,
retirer des passages, régler le son de chaque piste, puis exporter un **nouveau** fichier MP4. Tout est local, aucun changement côté serveur. L'éditeur fait
partie de la feature cargo `instant-replay` et n'existe que dans l'app Windows.

Ce qui dépend d'un appareil Windows (webview, encodeurs matériels, jeu en cours) est **non vérifié** : voir « Vérification ».

## Fonctionnement

- **Ouverture** : `montage_ouvrir` vérifie que le fichier existe, qu'il est dans le dossier des clips et que ffmpeg sait le lire, puis prépare dans `.estompe/` :
  une piste audio par fichier (`<clip>.mp4.piste<N>.m4a`, copie sans réencodage) et, si le clip n'est pas en H.264, un aperçu H.264 sans son, en 720p au plus,
  à la cadence du clip (`<clip>.mp4.apercu.mp4`, progression par `montage://preparation`). Ces fichiers sont réutilisés tant qu'ils sont plus récents que le clip,
  comptent dans le quota, suivent le clip quand il est renommé et sont supprimés avec lui. Un clip H.264 est lu tel quel.
- **Aperçu** : la vidéo est lue muette par la webview (`asset:`). Chaque piste audio est décodée (`decodeAudioData`) et jouée par Web Audio avec son gain
  (0 à 200 %, muet), calée sur la vidéo : le son démarre sur les évènements `playing` et `seeked`, s'arrête sur `pause`, `waiting` et `seeking`, et est recalé
  s'il s'écarte de plus de 40 ms. Les passages retirés sont sautés à chaque image présentée (`requestVideoFrameCallback`, `timeupdate` à défaut) ; la lecture
  s'arrête à la fin du dernier segment gardé. Les formes d'onde sont les crêtes des pistes décodées, ramenées à la plus forte.
- **Montage** : deux poignées (début et fin gardés), points de coupe à l'image courante, segment retiré ou restauré, volume et muet par piste. Tous les temps
  sont calés sur les images du clip (`ips`). Rogner au-delà d'un point de coupe retire les segments dépassés ; élargir depuis un segment retiré ajoute un segment
  gardé. Annuler et rétablir portent sur toutes ces opérations (200 états au plus), tant que l'éditeur est ouvert.
- **Projet** : chaque opération enregistre le montage dans `.estompe/<clip>.mp4.montage.json` (`montage_enregistrer`) ; il est relu à la réouverture. L'historique
  d'annulation n'est pas enregistré.
- **Export** : `montage_exporter` lance ffmpeg dans un processus à part, en priorité basse (`BELOW_NORMAL`, comme les autres appels à ffmpeg), et rend la main.
  La progression arrive par `montage://export` ; on peut fermer l'éditeur, la galerie montre l'export (progression, « Annuler ») puis le nouveau clip.
  Le fichier est écrit dans `.estompe/export-<pid>-<id>.part`, puis renommé `<clip> (montage).mp4` (` (montage 2)`, ` (montage 3)`… si le nom est pris). Son annexe
  porte `origine` (nom du clip source) : la galerie affiche l'étiquette « Montage » et « d'après … ». Le clip source n'est ouvert qu'en lecture.
- **Sortie** : toujours réencodée, H.264 + AAC (160 kbit/s, stéréo), résolution et cadence du clip, cadence constante, `+faststart`. Les segments gardés sont
  découpés à l'image près et recollés dans l'ordre (`fps`, `trim=start_frame/end_frame`, `atrim`, `concat`). Chaque piste non muette reçoit son volume puis les pistes
  sont additionnées en une seule (`amix`, `normalize=0` : 100 % reste 100 %, deux pistes fortes peuvent saturer, comme dans l'aperçu). Toutes les pistes muettes, ou
  clip sans son : le fichier n'a pas de piste audio. Débit vidéo : celui du clip (× 1,5 si le clip est en HEVC), entre 2 et 50 Mbit/s.
- **Encodeur** : les encodeurs matériels du ffmpeg embarqué sont essayés dans l'ordre (`h264_nvenc`, `h264_amf`, `h264_qsv`, `h264_mf`), puis l'encodeur logiciel
  (`libopenh264` ; le build LGPL embarqué n'a pas `libx264`). Un encodeur qui échoue, même en cours d'export, fait reprendre l'export avec le suivant ; un export
  terminé par l'encodeur logiciel est signalé (`logiciel: true` et message).
- **Erreurs** : fichier disparu ou illisible à l'ouverture (message, l'éditeur ne s'ouvre pas, la galerie est rechargée) ; quota déjà atteint ou place insuffisante
  sur le disque avant l'export (refus) ; disque plein ou échec pendant l'export (message, fichier partiel supprimé) ; quota franchi par le fichier exporté (il est
  gardé, message). Dans tous les cas le montage reste enregistré. Les fichiers `export-*.part` laissés par un arrêt brutal sont purgés par `replay_configurer`
  quand aucun export n'est en cours.

## Contrat avec le front

Commandes Tauri (objets en `camelCase`, erreurs en français). Sans la feature `instant-replay`, elles répondent « instant replay absent de ce build »
(`montage_exports` répond une liste vide). `chemin` suit la règle de la galerie : un `.mp4` placé directement dans le dossier des clips.

| Commande | Arguments | Réponse |
|---|---|---|
| `montage_ouvrir` | `chemin` | `{ chemin, nom, source, dureeS, largeur, hauteur, ips, codec, apercu, pistes: [{ nom, chemin }], projet }` |
| `montage_enregistrer` | `chemin`, `montage` | rien |
| `montage_exporter` | `chemin`, `montage` | l'état initial de l'export (le suivi se fait par évènement) |
| `montage_annuler` | `id` | rien ; ffmpeg est tué, le fichier partiel supprimé, puis `montage://export` passe à `annule` |
| `montage_exports` | | les exports en cours, dans le format de `montage://export` |

- `source` est le nom du fichier du clip (`<clip>.mp4`) ; `apercu` et `pistes[].chemin` sont des chemins lisibles par `asset:` ; `pistes[].nom` vaut « Jeu »,
  « Micro », puis « Piste N » (4 pistes au plus) ; `projet` est le montage enregistré, ou `null`.
- Le montage reçu n'est jamais cru : version, 1 à 500 segments, tous de la source ouverte, bornes finies, ordonnées et sans recouvrement, volumes de 0 à 2 pour la
  seule source ouverte ; à l'export, bornes dans la durée du clip (lue par ffmpeg) et au moins un segment gardé.

| Évènement | Charge |
|---|---|
| `montage://preparation` | `{ chemin, pct }` pendant la création de l'aperçu H.264 |
| `montage://export` | `{ id, source, nom, pct, etat, message, chemin, logiciel }` |

`etat` vaut `encours`, `fini`, `annule` ou `erreur`. `pct` vient de `out_time_us` (`-progress pipe:1` de ffmpeg) rapporté à la durée finale, plafonné à 99 ;
100 n'est émis qu'avec `fini`, une fois le fichier renommé. Il repart de 0 quand l'export reprend avec un autre encodeur. `source` est le chemin du clip source,
`chemin` celui du fichier exporté (avec `fini`), `message` le motif d'une erreur ou une remarque de fin (encodeur logiciel, quota).

## Format du projet

```json
{
  "version": 1,
  "segments": [
    { "source": "Jeu 2026-11-12 09-14-03.mp4", "debut": 0.5, "fin": 2, "garde": true },
    { "source": "Jeu 2026-11-12 09-14-03.mp4", "debut": 2, "fin": 3, "garde": false },
    { "source": "Jeu 2026-11-12 09-14-03.mp4", "debut": 3, "fin": 9.5, "garde": true }
  ],
  "volumes": { "Jeu 2026-11-12 09-14-03.mp4": [{ "volume": 0.5, "muet": false }, { "volume": 1, "muet": true }] }
}
```

- `segments` : liste ordonnée ; chaque segment désigne son clip source par son nom de fichier, ses bornes en secondes dans cette source, et s'il est gardé.
  Le résultat est la suite des segments gardés. Le premier `debut` et le dernier `fin` sont le rognage.
- `volumes` : par source, une entrée par piste audio (`volume` de 0 à 2, `muet`).
- En version 1, tous les segments viennent du clip ouvert et se suivent sans trou. À la lecture, `source` est ramenée au nom actuel du clip (il a pu être renommé),
  les temps sont recalés sur les images, et un projet illisible ou d'une autre version est ignoré (montage neuf).
- Types : `Montage`, `Segment`, `Volume` dans `apps/estompe/windows/src/montage.ts` et dans `src-tauri/src/replay/montage.rs`.

## Code

| Fichier | Rôle |
|---|---|
| `src/montage.ts` | modèle et logique pure : rogner, couper, retirer, restaurer, durée finale, temps du montage et de la source, saut des passages retirés, pas à pas, historique, lecture du projet, formes d'onde, raccourcis |
| `src/montage.svelte.ts` | état de l'éditeur (`editeur`), ouverture, enregistrement du projet, export, suivi des évènements dans `replay.travaux` |
| `src/ReplayEditeur.svelte` | vue de l'éditeur dans l'espace « Clips » : lecteur, ligne de temps, pistes, saisies, raccourcis, export |
| `src-tauri/src/replay/montage.rs` | validation, arguments ffmpeg, progression, préparation, export, annulation, commandes |

Raccourcis (affichés sous la ligne de temps, sans effet depuis un champ de saisie) : `Espace` lecture ou pause, `←` `→` image précédente ou suivante,
`C` point de coupe, `Suppr` retirer ou restaurer le segment choisi, `Ctrl + Z` annuler, `Ctrl + Y` ou `Ctrl + Maj + Z` rétablir. Les poignées et la règle se
règlent aussi au clavier (flèches : une image, avec `Maj` : une seconde, `Début`, `Fin`).

## Vérification

« Test » : test unitaire lancé en CI (`montage.test.ts` par vitest ; `cargo test --lib --features instant-replay replay`, job `estompe-tauri`). « ffmpeg réel » : test Rust qui
fabrique un clip de synthèse (H.264 ou HEVC, pistes stéréo et mono), le prépare et l'exporte avec le ffmpeg de la machine, ignoré avec un message s'il manque.
Ce ffmpeg n'est pas le binaire Windows embarqué, et l'encodeur y est `libx264`. « Non vérifié » : jamais exécuté sur un appareil Windows.

| Critère | État |
|---|---|
| C1 ouverture, lecture, pause, déplacement, pas à pas (H.264 et HEVC) | logique : test. Aperçu H.264 d'un clip HEVC : ffmpeg réel. Vue contrôlée dans Chromium avec des commandes simulées et un aperçu VP9. Dans la webview de l'app : non vérifié |
| C2 durée du fichier à une image près | découpe en images : test ; durée du fichier exporté : ffmpeg réel. Sur un clip du moteur : non vérifié |
| C3 trois segments dans l'ordre, raccords | graphe de filtres : test ; export de trois segments : ffmpeg réel (durée) ; pistes vidéo et audio de même longueur, relues par `ffprobe` à la main pendant le développement. Écoute et image aux raccords : non vérifié |
| C4 micro muet, jeu à 50 %, une seule piste | plan et graphe : test ; une piste, niveau du jeu divisé par deux, micro absent : ffmpeg réel. Aperçu identique à l'oreille : non vérifié |
| C5 H.264 + AAC, résolution et cadence d'origine | arguments : test ; relu sur le fichier exporté : ffmpeg réel. Lecture dans le client web après envoi : non vérifié |
| C6 original inchangé | contenu du clip comparé avant et après export : ffmpeg réel |
| C7 annuler et rétablir sur dix opérations | test |
| C8 annulation sans fichier partiel, montage conservé | ffmpeg réel (processus tué, aucun `.part`, projet présent) ; bouton et message : Chromium avec commandes simulées |
| C9 progression réelle, 100 % à la fin | analyse de `out_time_us`, 99 au plus avant la fin : test et ffmpeg réel |
| C10 jeu prioritaire pendant un export | priorité basse du processus ffmpeg ; non vérifié (un encodeur matériel partage la carte graphique avec le jeu) |
| C11 fichier disparu, disque plein | fichier absent ou illisible, quota, message « disque plein » de ffmpeg : test et ffmpeg réel. Disque réellement plein et contrôle de la place libre (Windows) : non vérifié |
| C12 logique couverte par des tests en CI | test |

Non vérifié par ailleurs : durée d'un export (aucun seuil fixé, à mesurer sur appareil) ; encodeurs matériels et `libopenh264` du binaire embarqué (options de débit) ;
reprise en logiciel après un échec matériel réel ; mémoire prise par le décodage des pistes d'un clip de 5 minutes (environ 115 Mo par piste stéréo) ;
en-têtes CORS du protocole `asset:` pour lire les pistes par `fetch`.
