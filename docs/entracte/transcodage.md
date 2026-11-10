# Lecture et transcodage

## Ouverture d'une lecture

`POST /titles/{id}/playback` : le client annonce ce qu'il sait lire (`support : {containers, video, audio}`, détecté par `canPlayType`),
la qualité voulue, la piste audio et la position de départ.

- **Lecture directe** si le fichier est lisible tel quel : qualité « Originale », conteneur et codecs annoncés par le client, première
  piste audio. Le fichier est servi avec les requêtes `Range`.
- **HLS** sinon : ffmpeg écrit une playlist et des segments MPEG-TS de 4 s dans un dossier temporaire propre au flux. Vidéo H.264, audio AAC
  stéréo 160 kb/s. En « Originale » d'une source H.264, la vidéo est recopiée sans réencodage (seul l'audio est converti).

Paliers manuels, jamais au-dessus de la source : Originale (12 Mb/s au plus si réencodée), 1080p (8 Mb/s), 720p (4 Mb/s), 480p (1,5 Mb/s).
Changer de piste audio, de palier ou de sous-titres rouvre un flux à la position courante (`start`) ; l'ancien est fermé et son
transcodage arrêté. En HLS, la position réelle vaut `offset` (renvoyé par l'API) plus le temps du lecteur.

Sous-titres texte (SRT, WebVTT, ASS, SSA), intégrés au fichier ou à côté (`Film.fr.srt`), convertis en WebVTT par ffmpeg puis nettoyés
(balises, scripts et liens retirés), mis en cache en base. Les sous-titres image (PGS, VobSub) ne sont pas proposés.

## Flux signés et limites

Le jeton d'un flux (dans l'URL) est signé (HMAC, `ENTRACTE_STREAM_SECRET`), lié à la personne et vérifié à chaque requête contre ses
droits ; une requête qui porte la session d'une autre personne est refusée. Aucune URL ne contient de chemin de fichier.

- `ENTRACTE_MAX_FLUX` lectures par personne, `ENTRACTE_MAX_TRANSCODAGES` transcodages au total, dossier temporaire limité à
  `ENTRACTE_TRANSCODE_MAX_GO` : au-delà, `503 serveur_charge`. Fermer un flux (`DELETE /streams/{id}`) libère sa place.
- Un flux sans requête depuis `ENTRACTE_INACTIVITE_SECONDES` (300) est fermé : ffmpeg est arrêté et ses fichiers supprimés.
  Le dossier temporaire est vidé au démarrage.

## Accélération matérielle

`ENTRACTE_TRANSCODE_MODE` : `cpu` (défaut, libx264), `vaapi` (Intel ou AMD), `qsv` (Intel Quick Sync), `nvenc` (NVIDIA).
Au démarrage, Entracte vérifie le mode demandé : périphérique présent (`ENTRACTE_TRANSCODE_DEVICE`, `/dev/dri/renderD128`), encodeur dans
ffmpeg, puis un court essai d'encodage. S'il échoue, il le dit dans ses journaux et dans Administration › état, et transcode sur le CPU.
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
4. Régler `ENTRACTE_TRANSCODE_MODE`, redéployer, vérifier Administration › état (« mode actif ») et `nvidia-smi` / `intel_gpu_top`
   pendant une lecture.
5. Mesurer la charge avec plusieurs lectures simultanées en 1080p, puis fixer `ENTRACTE_MAX_TRANSCODAGES`.
