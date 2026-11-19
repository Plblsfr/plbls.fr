# Lecture et transcodage

## Ouverture d'une lecture

`POST /titles/{id}/playback` : le client annonce ce qu'il sait lire (`support : {containers, video, audio}`, détecté par `canPlayType`),
la qualité voulue, la piste audio et la position de départ.

- **Lecture directe** si le fichier est lisible tel quel : qualité « Originale », conteneur et codecs annoncés par le client, première
  piste audio. Le fichier est servi avec les requêtes `Range`.
- **HLS** sinon : ffmpeg écrit une playlist et des segments de 4 s dans un dossier temporaire propre au transcodage. Audio AAC stéréo
  160 kb/s ; vidéo réencodée en H.264 (segments MPEG-TS), ou recopiée sans réencodage en « Originale » quand l'appareil sait la décoder
  (seul l'audio est converti) : source H.264 en MPEG-TS, source HEVC en fMP4 (`init.mp4` puis `segNNNNN.m4s`).

Recopie d'une source HEVC : l'appareil doit annoncer `hevc` pour une source 8 bits et `hevc10` pour une source 10 bits (HDR compris, dont
le rendu sur un écran SDR est laissé au navigateur) ; toute autre profondeur est réencodée. La profondeur est lue par ffprobe à l'analyse
du fichier ; pour un fichier analysé avant, elle est relue à la première lecture puis gardée. Si l'appareil ne décode finalement pas la vidéo
recopiée (erreur de décodage, ou flux sans image), le lecteur rouvre seul un flux réencodé à la même position et n'annonce plus ce codec
(`entracte.refus`, retenu sur l'appareil) ; une erreur de réseau ne retire aucun codec.

Départ en cours de titre (`start`) : une vidéo réencodée sur le CPU commence au multiple de 4 s qui précède la position demandée (voir
« Segments d'un réencodage »), une vidéo réencodée par le matériel exactement à la position demandée. Une vidéo recopiée ne peut commencer
que sur une image clé : Entracte cherche avec ffprobe celle qui précède la position, lance ffmpeg dessus (en visant une demi-seconde après
elle, loin de toute autre image clé, parce que ffmpeg ajuste lui-même la position qu'on lui donne) et renvoie cette image clé dans `offset`.
Le lecteur avance alors dans le flux jusqu'à `start` : l'image et le temps affiché sont ceux demandés. Une position à moins d'une seconde
après une image clé repart de l'image clé d'avant. Sans image clé trouvée (conteneur que ffprobe ne sait pas parcourir ainsi, comme le
MPEG-TS), la reprise reste approchée, à un groupe d'images près.
La playlist n'est alors servie qu'une fois écrit le segment qui suit celui du point de reprise (5 s d'attente au plus) : servie plus tôt,
le lecteur atteindrait la fin du premier segment avant de recharger la playlist et s'arrêterait quelques secondes.

Paliers manuels, jamais au-dessus de la source : Originale (12 Mb/s au plus si réencodée), 1080p (8 Mb/s), 720p (4 Mb/s), 480p (1,5 Mb/s).
Changer de piste audio, de palier ou de sous-titres rouvre un flux à la position courante (`start`) ; l'ancien est fermé et son
transcodage arrêté. En HLS, la position réelle vaut `offset` (renvoyé par l'API) plus le temps du lecteur.

Sous-titres texte (SRT, WebVTT, ASS, SSA), intégrés au fichier ou à côté (`Film.fr.srt`), convertis en WebVTT par ffmpeg puis nettoyés
(balises, scripts et liens retirés), mis en cache en base. Les sous-titres image (PGS, VobSub) ne sont pas proposés.
Une piste n'est extraite qu'une fois à la fois : les requêtes simultanées sur la même piste attendent le même ffmpeg, deux extractions
au plus tournent en même temps sur le serveur, et un échec est renvoyé tel quel pendant 60 s sans relancer ffmpeg.

## Segments d'un réencodage

Un réencodage sur le CPU découpe le titre sur une grille fixe : le segment `segNNNNN.ts` couvre toujours les secondes `4 × N` à
`4 × N + 4` du titre, quel que soit le point de départ du transcodage (`-ss` sur un multiple de 4 s, image clé forcée toutes les 4 s et aucune
autre, horodatages du titre gardés par `-copyts`, numérotation par `-start_number`). Deux transcodages du même fichier, à la même qualité et
avec la même piste audio produisent donc les mêmes segments.

- La liste HLS est complète dès l'ouverture (`VOD`, de la position demandée à la fin du titre, durées de 4 s) : le lecteur se déplace
  vers l'avant sans rouvrir de flux. Le dernier segment annoncé s'arrête une demi-seconde avant la durée du fichier, pour ne jamais
  annoncer un segment que ffmpeg n'écrira pas.
- Un segment demandé qui n'est pas encore écrit est attendu (30 s au plus) s'il est à moins de 8 s de ce que le transcodage a déjà produit.
  Plus loin, ou avant son point de départ, le transcodage du flux est déplacé : il rejoint un transcodage identique qui couvre ce segment,
  sinon ffmpeg est relancé à cette position (`503 serveur_charge` si `ENTRACTE_MAX_TRANSCODAGES` est atteint, le transcodage en cours reste
  alors en place).
- Une demande de préchargement (`?avance=1`, envoyée par le lecteur pour ce qu'il charge en avance) ne déplace jamais un transcodage : elle
  reçoit `404` et le lecteur réessaie plus tard. Seule la lecture décide où ffmpeg travaille.
- La recopie et les modes matériels gardent une liste écrite par ffmpeg au fil du transcodage, dont les segments dépendent du point de départ.

## Transcodage partagé

Un transcodage n'appartient pas à un flux : les flux qui demandent le même fichier, la même qualité et la même piste audio lisent les
segments du même ffmpeg quand la position demandée est couverte par un transcodage en cours (entre son point de départ et 8 s après ce
qu'il a déjà produit) ou terminé. Au salon cinéma, les spectateurs aux mêmes réglages ne coûtent donc qu'un transcodage ; une autre qualité
ou une autre piste en lance un autre. Rouvrir sa propre lecture à une position couverte garde le transcodage en cours.

- Chaque flux garde son jeton, lié à sa personne et vérifié à chaque requête : seul le dossier de segments est commun.
- ffmpeg est arrêté et ses fichiers supprimés quand son dernier flux est fermé ou expire.
- `ENTRACTE_MAX_TRANSCODAGES` compte les ffmpeg, pas les flux ; la limite de lectures par personne ne change pas.
- La réponse de lecture porte `cache` : deux flux de même valeur ont les mêmes noms de segments pour le même contenu (fichier, qualité,
  piste audio, et point de départ quand le découpage en dépend).
- Journaux : au démarrage et à l'arrêt de chaque transcodage, le codec de la source, recopie ou réencodage (et son mode), le palier,
  la durée produite, la vitesse (× temps réel) et le nombre de lecteurs au plus.

## Préchargement

Le lecteur charge la suite du titre en avance et le montre : sur la barre de position, la partie déjà chargée par l'appareil est plus claire
que le reste (plages `buffered` de la vidéo et segments gardés en mémoire, ramenés au temps du titre par `offset` en HLS). Le lecteur
« Regarder seul » affiche la durée chargée après la position en cours, et les deux lecteurs un indicateur quand la lecture attend des données.

- Lecture directe : la vidéo est créée avec `preload="auto"`, le navigateur charge donc avant le premier appui sur lecture ; la quantité
  gardée en avance reste décidée par le navigateur, quel que soit le réglage.
- HLS (hls.js) : au moins 60 s d'avance dans le tampon de la vidéo, et jusqu'à 100 Mo quand le débit le permet (10 min au plus, soit
  environ 100 s en 1080p, 200 s en 720p), contre 30 s et 60 Mo par défaut. Seules les 20 s déjà lues y sont gardées. Le navigateur borne
  ce tampon (de l'ordre de 150 Mo) : aller plus loin passe par le réglage ci-dessous.
- Réglage « Préchargement » (panneau « Audio et sous-titres » du lecteur, liste de la barre au salon cinéma), retenu sur l'appareil avec
  les autres préférences de lecture : « Normal » (le tampon ci-dessus), « Long » (15 min d'avance, 2 Go au plus) ou « Tout le film »
  (jusqu'à la fin, 16 Go au plus). Hors « Normal », le lecteur télécharge lui-même les segments HLS en avance
  (`web/estompe/src/blocks/regarder-ensemble/avance.ts`) : le premier seul, puis trois à la fois, le plus proche de la position d'abord ;
  après un échec il revient à un seul, et s'arrête au bout de trois échecs de suite. hls.js prend les segments chargés au lieu de les
  redemander, un saut dans la partie chargée est donc immédiat. Changer le réglage ne rouvre pas le flux.
- Les segments chargés en avance sont gardés sur le disque de l'appareil (OPFS, dossier `entracte-avance/<cache>`, `depot.ts`), ou en mémoire
  si l'appareil n'a pas ce stockage ou refuse l'écriture. Un segment est reconnu par la clé `cache` du flux et son nom : un flux rouvert
  avec la même qualité et la même piste audio (saut hors de la partie chargée puis retour, rechargement de la page) reprend ce qui est
  déjà là sans le redemander. Changer de qualité ou de piste audio repart de zéro. Les segments d'un titre sont supprimés à sa fermeture ;
  ceux d'une séance interrompue le sont à l'ouverture d'un lecteur, 24 h après leur dernière utilisation. Quand la limite du réglage est
  atteinte, les segments qui ne sont plus dans la liste du flux puis ce qui est à plus de 30 s derrière la position sont libérés.
  Pour une vidéo recopiée, la clé dépend du point de départ : ce qui est chargé n'est repris que si le flux rouvert rejoint le même transcodage.
- « Wi-Fi seulement » : hors Wi-Fi, le lecteur se comporte comme en « Normal », et reprend seul au retour du Wi-Fi. L'option n'est affichée
  que si la plateforme sait dire le type de connexion (`platform.network`, lu dans `navigator.connection.type` : app Android et Chrome sur
  Android) ; ailleurs elle est absente et le réglage choisi s'applique.

## Attente commune au salon cinéma

Quand le lecteur d'un participant attend des données plus d'une seconde pendant la lecture (ou n'a pas fini de charger le titre), il le
signale (`POST /voice/{cid}/cinema/ready` avec `{ready: false, wait: true}`) : la séance passe en pause pour tous à la position atteinte,
et le lecteur affiche qui est attendu. Elle reprend seule quand chaque personne attendue a au moins 5 s de vidéo chargée devant elle, ou
a quitté le salon. Une lecture ou une pause demandée par l'hôte annule l'attente ; un lecteur en erreur ne met personne en pause.

## Flux signés et limites

Le jeton d'un flux (dans l'URL) est signé (HMAC, `ENTRACTE_STREAM_SECRET`), lié à la personne et vérifié à chaque requête contre ses
droits ; une requête qui porte la session d'une autre personne est refusée. Aucune URL ne contient de chemin de fichier.

- `ENTRACTE_MAX_FLUX` lectures par personne, `ENTRACTE_MAX_TRANSCODAGES` transcodages (ffmpeg) au total, dossier temporaire limité à
  `ENTRACTE_TRANSCODE_MAX_GO` : au-delà, `503 serveur_charge`. Fermer un flux (`DELETE /streams/{id}`) libère sa place.
- La taille du dossier temporaire est revérifiée toutes les 10 s : tant qu'elle dépasse `ENTRACTE_TRANSCODE_MAX_GO`, le transcodage
  démarré le plus tôt est arrêté, ses fichiers supprimés et ses flux fermés (chaque lecteur reçoit `401` et doit rouvrir une lecture).
- Un flux sans requête depuis `ENTRACTE_INACTIVITE_SECONDES` (300) est fermé : ffmpeg est arrêté et ses fichiers supprimés.
  Le dossier temporaire est vidé au démarrage.

## Diffusion sur une télé

Le lecteur de l'app propose « Diffuser » quand le navigateur a l'API Remote Playback (`video.remote` : Chromecast depuis Chrome et Edge,
AirPlay depuis Safari) ; aucun script tiers n'est chargé. Le navigateur ouvre son choix d'appareil, puis la télé lit elle-même l'URL du
flux, que son jeton signé suffit à ouvrir. La lecture, la pause et la position restent pilotées depuis l'app, qui continue d'enregistrer la progression.

- Un flux HLS lu par hls.js ne se diffuse pas. À l'appui, le lecteur demande d'abord au navigateur si un appareil existe (sonde sans
  chargement) : sans appareil, la lecture en cours n'est pas touchée. Sinon il rouvre un flux à la position en cours dans le lecteur natif
  du navigateur (s'il lit le HLS lui-même), puis ouvre le choix de l'appareil ; si le navigateur refuse alors faute de geste récent, un
  second appui suffit. Le lecteur natif ne sait pas se déplacer dans un flux en cours d'écriture : chaque saut rouvre un flux, et la fin
  de la diffusion revient à hls.js.
- Les routes `/stream/…` répondent `Access-Control-Allow-Origin: *` (sans `Allow-Credentials`) à une origine hors de
  `ENTRACTE_ALLOWED_ORIGINS` : le récepteur d'une télé lit playlists et segments depuis sa propre origine. Seul le jeton de l'URL ouvre
  le flux ; aucune autre route n'est concernée.
- Les sous-titres ne suivent pas sur la télé. Sans cette API, ou si le navigateur ou la webview la désactive, le bouton n'apparaît pas.

## Accélération matérielle

`ENTRACTE_TRANSCODE_MODE` : `cpu` (défaut, libx264), `vaapi` (Intel ou AMD), `qsv` (Intel Quick Sync), `nvenc` (NVIDIA).
Au démarrage, Entracte vérifie le mode demandé : périphérique présent (`ENTRACTE_TRANSCODE_DEVICE`, `/dev/dri/renderD128`), encodeur dans
ffmpeg, puis un court essai d'encodage. S'il échoue, il le dit dans ses journaux et dans l’en-tête d’Administration (état des services), et transcode sur le CPU.
Un transcodage matériel qui échoue au démarrage (source que le matériel ne décode pas) est relancé sur le CPU pour ce flux seulement, et signalé ; les suivants restent sur le matériel.

| Mode | Décodage, mise à l'échelle, encodage |
|---|---|
| `vaapi` | `-hwaccel vaapi`, `scale_vaapi`, `h264_vaapi` |
| `qsv` | `-hwaccel qsv`, `scale_qsv`, `h264_qsv` |
| `nvenc` | `-hwaccel cuda`, `scale_cuda`, `h264_nvenc` |

Les lignes de commande de chaque mode sont couvertes par les tests de `api/entracte/src/transcodage.rs`.

### Mise en place (plbls.fr-infra)

1. Identifier le matériel du serveur (`lspci | grep -iE 'vga|3d'`, `ls /dev/dri`) : il fixe le mode à valider en premier.
2. Intel ou AMD (`vaapi`, `qsv`) : passer le périphérique au service `entracte-api` (`devices: ["/dev/dri/renderD128:/dev/dri/renderD128"]`)
   et donner au conteneur le groupe propriétaire du périphérique (`group_add: ["<gid de render>"]`, `stat -c %g /dev/dri/renderD128`).
   L'image porte les pilotes libres (`mesa-va-drivers`, `intel-media-va-driver`).
3. NVIDIA (`nvenc`) : NVIDIA Container Toolkit sur l'hôte, puis `deploy.resources.reservations.devices` (`driver: nvidia`,
   `capabilities: [gpu, video]`) sur le service.
4. Régler `ENTRACTE_TRANSCODE_MODE`, redéployer, vérifier l’en-tête d’Administration (état des services) (« mode actif ») et `nvidia-smi` / `intel_gpu_top`
   pendant une lecture.
5. Mesurer la charge avec plusieurs lectures simultanées en 1080p, puis fixer `ENTRACTE_MAX_TRANSCODAGES`.
