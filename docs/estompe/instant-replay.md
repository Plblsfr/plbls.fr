# Instant replay (app Windows)

L'app Windows garde en mémoire les dernières secondes ou minutes d'un jeu et les enregistre en clip MP4 sur un raccourci clavier (issue #158).
La fonction est en **bêta**, désactivée par défaut, et n'existe que sous Windows. Aucun changement côté serveur.

Ce document décrit le moteur natif (Rust) et son contrat avec le front. Tout ce qui touche le matériel (carte graphique, Windows Graphics Capture,
Media Foundation, WASAPI) est **non vérifié** : le code compile pour Windows mais n'a jamais tourné sur un appareil. Voir « Vérification ».

## Build

Feature cargo `instant-replay` de `apps/estompe/windows/src-tauri`. Elle compile pour `x86_64-pc-windows-gnu` (mingw, build par défaut) et pour MSVC,
seule ou avec `livekit-native` et `overlay-natif`. Les scripts `release-windows.sh` et `build-test-windows.sh` l'activent ; `INSTANT_REPLAY=0` la retire.

```sh
cd apps/estompe/windows && PATH=../../../node_modules/.bin:$PATH tauri build -- --features instant-replay
```

Sans la feature, les commandes `replay_*` existent et renvoient l'erreur « instant replay absent de ce build » ; `replay_disponible` répond
`disponible: false`. Avec la feature hors Windows (tests), seule la galerie fonctionne.

Dépendances ajoutées par la feature : `windows` 0.62 (déjà dans l'arbre par `wasapi` et `xcap`, features au plus juste), `wasapi` 0.24 (partagé avec
`livekit-native`), `nnnoiseless` 0.5 (RNNoise en Rust pur, licence BSD-3-Clause, sans ses dépendances de binaire).

## Chaîne de traitement

Aucune image ne passe par le processeur. Fichiers dans `apps/estompe/windows/src-tauri/src/replay/` (`win/` : code Windows seulement).

1. **Détection** (`jeux/`, `win/mod.rs`) : un crochet `SetWinEventHook` (premier plan, réduction, déplacement de la fenêtre au premier plan) prévient le moteur,
   qui classe la fenêtre avec `jeux::cible` : liste des jeux connus, plein écran sans bordure, jeux ajoutés à la main (chemin de l'exécutable), jeux exclus
   (nom de l'exécutable). Le crochet n'existe que si la fonction est activée. La fermeture du jeu est vue par une attente sur son processus.
2. **Capture** (`win/capture.rs`) : Windows Graphics Capture de la **fenêtre** du jeu, en texture D3D11, sur un appareil D3D11 propre au moteur. Rien d'autre
   n'est capturé : ni le bureau, ni l'overlay d'Estompe, ni une fenêtre posée par-dessus. Curseur selon le réglage, bordure jaune retirée quand Windows le permet.
3. **Conversion** (`win/gpu.rs`) : mise à l'échelle et BGRA vers NV12 (BT.709, 16-235) par le Video Processor D3D11. Écran en HDR : capture en 16 bits flottants,
   puis un nuanceur ramène l'image en SDR (blanc SDR de l'écran lu par `DisplayConfigGetDeviceInfo`, compression des hautes lumières, gamma sRGB).
4. **Encodage** (`win/encodeur.rs`) : MFT matériel Media Foundation (AMF, NVENC ou QuickSync par la même API), débit constant, une image clé par seconde,
   sans images B. **Aucun repli logiciel** : sans MFT matériel, la fonction est indisponible.
5. **Cadence** (`cadence.rs`, `win/mod.rs`) : un minuteur haute résolution émet une image à chaque échéance. Sans nouvelle image du jeu, la dernière est
   répétée ; au-delà de 3 images de retard, les suivantes sont comptées perdues. Chaque image garde son horodatage réel : durée et synchronisation restent justes.
6. **Son** (`win/son.rs`, `son_wasapi.rs`) : WASAPI « process loopback » sur le processus du jeu et ses enfants (piste « Jeu », stéréo), et, si le réglage est
   actif, le micro (piste « Micro », mono) avec RNNoise et le gain. Les silences sont comblés d'après l'horloge commune avec la vidéo. Chaque piste est encodée
   en AAC par le MFT de Windows. Micro éteint : aucun périphérique de capture n'est ouvert.
7. **Tampon** (`tampon.rs`) : seules les unités compressées sont gardées, en RAM. Éviction par groupe d'images entier, plafond en durée et en octets.
8. **Sauvegarde** (`ecriture.rs`, `mp4.rs`) : un instantané du tampon (sans copie des données) part dans une file ; un fil à part, en priorité basse, écrit le MP4
   (`moov` en tête, pistes entrelacées par seconde), puis l'annexe et la vignette. Le tampon continue de tourner.

Le partage d'écran natif (`screen_native.rs`) et le moteur partagent la boucle de capture WASAPI de `son_wasapi.rs`.

### États

| État | Sens |
|---|---|
| `inactif` | fonction désactivée, ou aucun jeu détecté |
| `arme` | le tampon tourne : capture, encodeur et son ouverts |
| `pause` | le jeu n'est plus au premier plan, ou le partage d'écran a la priorité : tout est fermé, **le tampon est gardé**, reprise automatique |
| `indisponible` | pas d'encodeur matériel, ou capture refusée pour ce jeu ; `raison` dit pourquoi |

Le tampon est vidé à la fermeture du jeu, au changement de jeu, à la désactivation, et quand un réglage vidéo ou audio change. Si l'image du jeu change de
taille, la session redémarre et le passé est perdu (évènement `replay://erreur`, code `image-changee`).

Jeu qui refuse la capture : l'état passe à `indisponible` (code `capture-refusee`), sans contournement. Un jeu qui ne livre aucune image en 5 secondes
d'échéances (plein écran exclusif) est traité de même.

### Réglages effectifs

- Résolution : réduction seulement, rapport conservé, dimensions paires (`reglages::dimensions`).
- Cadence : plafonnée par le rafraîchissement de l'écran du jeu (`EnumDisplaySettingsW`) et par l'encodeur (`MF_VIDEO_MAX_MB_PER_SEC` lu sur le MFT ; s'il ne
  l'annonce pas, la limite du niveau 5.2 pour H.264 et 6.2 pour HEVC).
- Codec : H.264 si l'encodeur H.264 accepte la résolution et la cadence, sinon HEVC.
- Débit : table de l'issue (qualité haute), 65 % en moyenne, 40 % en basse ; interpolé selon le nombre de pixels hors des quatre résolutions de la table.
- Durée : le tampon ne dépasse pas un quart de la RAM libre ; `dureeMaxS` le reflète.
- Un réglage hors de portée n'est pas refusé par `replay_configurer` : le moteur applique le plus proche (résolution native, cadence inférieure, durée plafonnée)
  et `replay_etat` donne les valeurs réelles.

### Carte graphique retenue

L'adaptateur est le premier, dans l'ordre « haute performance » de Windows (`EnumAdapterByGpuPreference`), qui possède un encodeur matériel : c'est celui que
Windows attribue aux jeux sur un portable à deux cartes. La capture et l'encodage se font sur cet adaptateur. **Non vérifié** sur un appareil à deux cartes.

## Fichiers

- Dossier des clips : `Vidéos/Estompe` par défaut. Rien n'y est créé avant la première sauvegarde.
- Clip : `<Jeu> AAAA-MM-JJ HH-MM-SS.mp4` (heure locale, caractères interdits par Windows retirés, ` (2)` en cas de collision).
  Vidéo H.264 ou HEVC, une piste AAC « Jeu », et une piste AAC « Micro » si le micro est actif.
- Sous-dossier caché `.estompe/` : `<clip>.mp4.json` (jeu, date, durée, codec, dimensions, cadence, pistes audio, et `origine` pour un montage exporté),
  `<clip>.mp4.jpg` (vignette, produite par le ffmpeg embarqué), `envoi/` (fichiers réencodés pour l'envoi, purgés à chaque `replay_configurer`), fichiers
  `.part` pendant une écriture, et les fichiers de l'éditeur ([`editeur-clips.md`](editeur-clips.md) : projet, aperçu, pistes audio extraites). Renommer ou
  supprimer un clip renomme ou supprime tous ses fichiers de `.estompe/`.
  Un MP4 déposé à la main apparaît dans la galerie sans ces informations.
- Quota : somme des clips et de `.estompe/`. Quota déjà atteint : la sauvegarde est refusée (`quota`). Clip qui fait franchir le quota : il est écrit, puis
  `replay://erreur` `quota` prévient. **Aucun clip n'est jamais supprimé par le moteur** : `replay_stockage` propose les plus anciens, le front confirme.
- Le dossier des clips est ajouté au scope du protocole `asset:` à l'exécution (`allow_directory`) ; `tauri.conf.json` garde un scope vide.

## Contrat avec le front

Commandes Tauri. Les objets sont en `camelCase`. Toute commande peut échouer avec un message en français.

| Commande | Arguments | Réponse |
|---|---|---|
| `replay_disponible` | | `{ disponible, raison, encodeur }` |
| `replay_options` | `reglages` | `{ resolutions: [{ id, disponible, raison }], cadences: [{ ips, disponible, raison }], dureeMaxS, raisonDuree, codec, debitKbps, memoireOctets }` |
| `replay_configurer` | `reglages` | rien |
| `replay_etat` | | `{ etat, jeu, raison, codec, largeur, hauteur, ips, microParDefaut }` |
| `replay_sauvegarder` | | rien (le résultat arrive par évènement) |
| `replay_partage` | `actif`, `suspendre` (optionnel) | rien |
| `replay_clips` | | `[{ chemin, nom, jeu, date, dureeS, tailleOctets, vignette, codec, largeur, hauteur, ips, pistesAudio, origine }]`, du plus récent au plus ancien |
| `replay_renommer` | `chemin`, `nom` | nouveau chemin |
| `replay_supprimer` | `chemins` | rien |
| `replay_ouvrir_dossier` | | rien |
| `replay_stockage` | | `{ utiliseOctets, quotaOctets, depasse, plusAnciens }` |
| `replay_vers_h264` | `chemin`, `tailleMaxOctets` (ou `null`) | chemin d'un fichier H.264 + AAC |
| `replay_choisir_executable`, `replay_choisir_dossier` | | chemin, ou `null` si annulé |

`reglages` (validés côté Rust, une valeur hors bornes fait échouer la commande) :

| Champ | Valeurs | Défaut |
|---|---|---|
| `actif` | booléen | `false` |
| `resolution` | `"720p"`, `"1080p"`, `"1440p"`, `"4k"`, `"native"` | `"1080p"` |
| `ips` | 30, 60, 120, 144 | 60 |
| `dureeS` | 15 à 300 | 30 |
| `qualite` | `"basse"`, `"moyenne"`, `"haute"` | `"haute"` |
| `micro` | booléen | `false` |
| `microLibelle` | libellé du micro choisi dans la webview (`MediaDeviceInfo.label`), `null` pour le micro par défaut | `null` |
| `microReduction` | `"off"`, `"standard"`, `"strong"` (valeurs de `prefs.noise`) ; hors `"off"`, RNNoise est appliqué | `"strong"` |
| `microGain` | multiplicateur de 0 à 3 (`prefs.micGain / 100`) | 1 |
| `curseur` | booléen | `false` |
| `dossier` | chemin absolu, `null` pour `Vidéos/Estompe` | `null` |
| `quotaOctets` | 1 Go au moins | 20 Go |
| `jeuxAjoutes` | chemins complets d'exécutables (200 au plus) | `[]` |
| `jeuxExclus` | noms d'exécutables, sans dossier (200 au plus) | `[]` |

Précisions :

- `replay_configurer` est à appeler au lancement et à chaque changement de réglage. Il démarre le moteur ; tant qu'il n'a pas été appelé avec `actif: true`,
  ni crochet, ni carte graphique, ni micro ne sont touchés. Changer un réglage vidéo ou audio vide le tampon (au front de prévenir avant).
- `replay_options` et `replay_disponible` lisent le matériel : ils échouent ou répondent `disponible: false` sans encodeur matériel. La source prise pour les
  plafonds est la fenêtre du jeu en cours, sinon l'écran principal.
- `replay_etat` : `etat` vaut `"inactif"`, `"arme"`, `"pause"` ou `"indisponible"` ; `codec`, `largeur`, `hauteur`, `ips` ne sont renseignés que dans l'état `arme`.
  `microParDefaut` est vrai quand `microLibelle` n'a correspondu à aucun périphérique et que le micro par défaut de Windows a été pris.
- `replay_partage(actif, suspendre)` : à appeler au début (`actif: true`) et à la fin (`actif: false`) de tout partage d'écran. Tampon et partage tournent
  ensemble ; si l'encodeur refuse ou perd sa session pendant un partage, le tampon passe en `pause` au lieu d'`indisponible`. `suspendre: true` force la pause
  tout de suite (i/s du partage en chute). `actif: false` relance le tampon.
- Galerie : `chemin` doit désigner un `.mp4` placé directement dans le dossier des clips (chemin canonisé, liens suivis) ; tout autre chemin est refusé.
  `date` est une date ISO locale sans fuseau pour un clip d'Estompe, en UTC avec `Z` pour un fichier sans annexe. Sans annexe : `jeu`, `codec` et `vignette`
  valent `null`, les nombres valent 0. `origine` est le nom du clip source d'un montage exporté, `null` sinon.
- `replay_vers_h264` : renvoie le clip lui-même s'il est déjà en H.264, à une seule piste audio et sous `tailleMaxOctets`. Sinon produit un fichier dans
  `.estompe/envoi/` (lisible par `asset:`) avec le ffmpeg embarqué : les deux pistes audio sont mélangées en une, la vidéo est copiée si elle convient, ou
  réencodée par un encodeur matériel au débit qui tient dans la taille (`galerie::debit_pour_taille`, hauteur réduite sous 6 Mbit/s). Échoue si le clip est
  trop long pour la taille, ou si le résultat la dépasse encore.
- Le raccourci global (`Alt+F10` par défaut) est enregistré par le front, qui appelle `replay_sauvegarder`. Deux appels rapprochés donnent deux clips.

Évènements :

| Évènement | Charge |
|---|---|
| `replay://etat` | celle de `replay_etat`, à chaque changement |
| `replay://clip` | `{ chemin, nom, jeu, dureeS, tailleOctets }` |
| `replay://erreur` | `{ code, message }` |

Codes d'erreur : `tampon-vide`, `disque-plein`, `dossier-inaccessible`, `quota`, `ecriture` (clip non enregistré, sauf `quota` après un clip écrit) ;
`capture-refusee`, `gpu-sature`, `sans-encodeur`, `gpu`, `minuteur`, `tampon` (arrêt du tampon) ; `image-changee` (redémarrage) ;
`son-indisponible`, `micro-indisponible` (la piste manque, le tampon tourne). `gpu-sature` est aussi émis, sans arrêt, quand plus de 20 % des images sont
perdues sur 10 secondes.

L'overlay natif (`overlay_natif`) ne sait afficher que les participants du vocal : l'indicateur, « Clip enregistré » et les messages d'erreur sont à afficher
par le front à partir de ces évènements.

## Interface

Tout le front de la fonction est dans `apps/estompe/windows/src/` ; le frontend commun ne la connaît que par `Platform` (un onglet de `slots.settingsPages`,
un espace de `spaces`, une fenêtre de `windows`). Sur un build sans la feature (`replay_etat` répond la raison « instant replay absent de ce build »), l'onglet
et l'espace sont masqués et aucune autre commande `replay_*` n'est appelée.

| Fichier | Rôle |
|---|---|
| `replay.ts` | types du contrat et logique pure : défauts, lecture des réglages enregistrés, réglages envoyés au moteur, décision d'envoi, tri de la galerie, messages de l'overlay |
| `replay.svelte.ts` | état (`replay`), enregistrement des réglages, `replay_configurer`, écoute des évènements, raccourci global, signal de partage d'écran, actions de la galerie, envoi |
| `SettingsReplay.svelte` | Paramètres › Instant replay |
| `ReplayNav.svelte`, `ReplayGalerie.svelte`, `ReplayEnvoi.svelte` | espace « Clips » : menu latéral, galerie, envoi dans un salon |
| `montage.ts`, `montage.svelte.ts`, `ReplayEditeur.svelte` | éditeur de clips ([`editeur-clips.md`](editeur-clips.md)) |
| `replay-overlay.svelte.ts`, `ReplayOverlay.svelte` | fenêtre d'overlay `replay-overlay` (capacité `src-tauri/capabilities/replay-overlay.json`) |

- **Réglages** : Paramètres › Instant replay (mention « bêta » dans l'onglet et dans la page). Ils sont enregistrés sur l'appareil (`localStorage`, clé
  `estompe.replay`) avec la version de l'app : au lancement, si la version a changé, la fonction repart désactivée et les autres réglages sont gardés.
  Le micro, la réduction de bruit et le volume viennent de Paramètres › Voix et vidéo (`prefs.micId`, `prefs.noise`, `prefs.micGain`) et ne sont lus que si
  « Enregistrer mon micro » est allumé ; le libellé du micro est celui de `enumerateDevices()`. Chaque changement renvoie `replay_configurer` (un envoi
  identique au précédent est ignoré).
- **Choix hors de portée** : résolutions et cadences refusées par `replay_options` sont grisées, la raison au survol et rappelée sous les choix ; la durée est
  plafonnée à `dureeMaxS` avec `raisonDuree` ; le codec, le débit et la mémoire du tampon annoncés sont affichés. Sans encodeur matériel
  (`replay_disponible`), la page affiche la raison et l'interrupteur d'activation est bloqué.
- **Raccourci** : `Alt+F10` par défaut, modifiable dans la page (vérifié comme les autres raccourcis globaux). Il n'est enregistré auprès de Windows que si
  la fonction est activée, et appelle `replay_sauvegarder`.
- **Galerie** : espace « Clips » de la barre des serveurs, visible dès que la fonction est activée ou qu'un clip existe ; un filtre par jeu dans le menu latéral.
  Lire (lecteur intégré, message si la webview ne lit pas le fichier, HEVC notamment), renommer, supprimer et « supprimer les plus anciens » quand le quota est
  atteint (toujours avec confirmation), ouvrir le dossier, espace utilisé. Elle se recharge sur `replay://clip`. Les actions d'un clip sont la liste `ACTIONS`
  de `ReplayGalerie.svelte`, les travaux en cours la liste `replay.travaux` (envois, et exports de l'éditeur avec leur progression et leur annulation).
  « Modifier » ouvre l'éditeur de clips dans le même espace : voir [`editeur-clips.md`](editeur-clips.md).
- **Envoyer dans un salon** : choix d'un salon texte où l'on peut joindre un fichier, ou d'une conversation privée, puis envoi par `uploadAttachment` et
  `app.send`. `decisionEnvoi` choisit : tel quel (H.264, une piste audio, sous la limite), `replay_vers_h264` (HEVC, deux pistes ou codec inconnu), réencodage
  plus léger proposé au-delà de la limite (`max_video_size` de `GET /me`, passé en `tailleMaxOctets`), refus quand le débit qui tiendrait est sous 300 kbit/s.
- **Partage d'écran** : `replay_partage` suit `voice.screen` (partage natif ou de la webview) tant que la fonction est activée. `suspendre` n'est pas utilisé :
  le front ne mesure pas les i/s du partage.
- **En jeu** : la fenêtre `replay-overlay` (transparente, traversée par les clics, dans le coin haut droit de l'écran du jeu, haut gauche si l'overlay des
  participants occupe le haut droit) montre une pastille « Replay » tant que l'état est `arme`, puis « Clip enregistré », les pauses, les indisponibilités
  (`raison` de `replay://etat`) et les messages de `replay://erreur`. Un message disparaît après la durée d'affichage des messages de l'overlay
  (`prefs.msgSeconds`, même minuterie que l'overlay des messages). La fenêtre n'existe pas tant qu'il n'y a rien à montrer. Une erreur est aussi affichée
  dans la fenêtre principale.

## Journaux

Avec le mode debug, source `replay` : carte graphique et encodeurs trouvés, puis à chaque armement l'encodeur retenu, le codec, la résolution et la cadence
effectives, le débit, la durée ; toutes les 10 secondes les images émises et perdues et la taille du tampon ; chaque changement d'état avec sa raison ;
chaque clip enregistré ou refusé.

## Vérification

Critères de l'issue. « Test » : test unitaire lancé en CI (`cargo test --lib --features instant-replay replay`, job `estompe-tauri`). « Non vérifié » : code écrit,
compilé pour Windows, jamais exécuté.

| Critère | État |
|---|---|
| C1 durée du clip à 1 s près | découpe à l'image clé (`tampon`) et MP4 (`mp4`, relu par `ffprobe` pendant le développement) : test. Sur un jeu : non vérifié |
| C2 résolution, cadence, moins de 1 % d'images perdues | calculs : test. Compteur dans les journaux : non vérifié |
| C3 perte d'i/s de 5 % au plus | non vérifié |
| C4 aucune écriture avant sauvegarde, mémoire sous le plafond | plafond du tampon : test. Rien n'écrit sur le disque avant une sauvegarde (lecture du code) ; 30 minutes de jeu : non vérifié |
| C5 rien ne tourne sans jeu au premier plan | automate : test. Fermeture réelle des sessions : non vérifié |
| C6 une ou deux pistes audio | MP4 et annexe : test. Micro jamais ouvert s'il est éteint (lecture du code) ; sur appareil : non vérifié |
| C7 micro hors vocal, réduction de bruit | RNNoise et gain : test. Capture WASAPI : non vérifié |
| C8 son du jeu seul | non vérifié (même capture que le partage d'écran natif) |
| C9 synchronisation à 40 ms | horodatage et comblement des silences : test. Sur un clip réel : non vérifié |
| C10 indisponible sans encodeur, choix grisés avec raison | raisons : test. Page des réglages : rendu contrôlé avec des commandes simulées. Lecture du matériel : non vérifié |
| C11 HEVC au-delà de H.264, envoi en H.264 | choix du codec, `hvcC`, arguments ffmpeg : test. Encodeur HEVC et réencodage réels : non vérifié |
| C12 jeux ajoutés et exclus | règle : test. Détection réelle : non vérifié |
| C13 HDR | non vérifié |
| C14 curseur, overlay absent | non vérifié |
| C15 partage d'écran prioritaire | automate : test. Refus réel d'une seconde session : non vérifié |
| C16 deux appuis, deux clips | file d'écriture : test. Effet sur les i/s : non vérifié |
| C17 quota et disque plein | test (aucune suppression, codes d'erreur). Avertissement et suppression confirmée dans la galerie : rendu contrôlé avec des commandes simulées |
| C18 désactivé par défaut, mention « bêta » | défauts et désactivation après une mise à jour : test (Rust et `replay.test.ts`). Mention « bêta » : dans l'onglet et la page des réglages |
| C19 logique sans GPU testée en CI | test |

Points d'attention pour le premier essai sur appareil :

- texture de Windows Graphics Capture donnée telle quelle au Video Processor (certains pilotes exigent une copie) ;
- MFT matériel : ordre des réglages, présence des paramètres de séquence, une unité AAC par échantillon de sortie ;
- retard propre à l'encodeur AAC (les horodatages de sortie du MFT sont repris tels quels) ;
- sans annulation d'écho, le son des enceintes peut repasser dans la piste micro ;
- profil de build `opt-level = "s"` : coût de RNNoise et de l'écriture MP4 à mesurer.
