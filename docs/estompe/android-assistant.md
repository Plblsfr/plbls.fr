# Assistant vocal d'Estompe (Android)

Un assistant réveillé par le mot « Estompe », qui lit les messages, envoie un SMS et pilote la musique, sans toucher le téléphone.
La reconnaissance, la compréhension et la synthèse restent sur l'appareil : aucun service Google, aucune donnée vocale envoyée sur le réseau.
Il complète le [mode conduite](android-voiture.md), dont il réutilise la synthèse vocale et la dictée Vosk.

## Activer

Paramètres › **Assistant vocal** (onglet marqué « bêta », présent dans l'app Android seulement) :

| Réglage | Effet |
|---|---|
| Écoute | « Éteint » (par défaut), « En mode conduite seulement », « Tout le temps » |
| Applications lues | messageries dont les notifications sont lues, en plus des SMS |
| Application de musique par défaut | celle qui reçoit « mets… » quand aucune app n'est nommée ; sinon la dernière utilisée |
| Compréhension étendue (IA locale) | modèle de langage facultatif, utilisé seulement quand aucun ordre n'est reconnu |

Autorisations, demandées depuis ce même écran :

- **Micro** et **notifications** : nécessaires à l'écoute ;
- **SMS** et **contacts** : lecture et envoi de SMS, noms du carnet ;
- **Accès aux notifications** (réglage système) : lecture des autres messageries et pilotage de la musique ;
- **Affichage par-dessus les autres applications** (réglage système, facultatif) : bandeau à l'écran quand l'assistant s'active,
  et ouverture de l'application de musique quand Estompe n'est pas à l'écran ;
- **Batterie sans restriction** (réglage système, conseillé) : sans elle, certains téléphones arrêtent Estompe en arrière-plan ou écran éteint.

Le modèle de dictée français (~40 Mo) est celui du mode conduite : il doit être téléchargé.
Une synthèse vocale française doit être installée (RHVoice, eSpeak…) : sans elle l'assistant refuse tout ordre, puisqu'il ne pourrait pas relire un SMS avant l'envoi.

L'APK étant installé hors magasin, Android 13 et plus peut griser l'accès aux notifications et les permissions SMS : ouvrir la fiche de l'application
dans les réglages système, menu ⋮, « Autoriser les paramètres restreints », puis recommencer.

Écran éteint ou Estompe en arrière-plan, l'écoute continue : le service garde le processeur éveillé (verrou d'éveil partiel) tant qu'il tourne,
ce qui consomme de la batterie.
Tant que l'écoute est active, une notification permanente « Assistant vocal » le signale et l'indicateur de micro d'Android reste affiché.
Android interdit de rouvrir le micro depuis l'arrière-plan : après un redémarrage du téléphone, ou quand le mode conduite démarre seul
à la connexion Bluetooth, la notification « Assistant vocal en pause » demande d'ouvrir Estompe pour relancer l'écoute.

## Annonce des messages qui arrivent

Réglage « Annoncer les messages qui arrivent » (éteint par défaut). Quand un message d'une application cochée arrive pendant que l'écoute est active,
l'assistant dit « Message de Marie sur Signal. Je le lis ? » et écoute quelques secondes : « oui » le lit, « réponds » ouvre la dictée d'une réponse,
« non » ou le silence referment. Le contenu n'est jamais lu sans cette réponse. Plus tard, « Estompe, lis-le » lit le dernier message annoncé.
Une annonce au plus toutes les vingt secondes, aucune pendant un ordre, une dictée ou un appel vocal ; les conversations de groupe sont ignorées.

## Android Auto

Android Auto ne laisse aucune application tierce jouer le rôle d'assistant vocal (bouton du volant et micro de l'écran sont réservés à celui de Google) et n'accepte,
pour un APK installé hors Play Store, que les applications média et les notifications de messagerie. Estompe se déclare donc comme application média (`CarService`) :
son icône apparaît sur l'écran de la voiture avec trois actions, « Donner un ordre », « Lire mes messages » et « Lire le dernier message », et l'écran de lecture
affiche l'état de l'assistant (« À l'écoute », la phrase entendue, la réponse). L'écoute doit déjà tourner sur le téléphone : Android interdit d'ouvrir le micro
depuis l'arrière-plan. Le mot d'éveil reste capté par le micro du téléphone. Pendant un ordre, les touches lecture/pause du volant vont à Estompe ; elles reviennent
à l'application de musique dès qu'il est terminé. Activer « Sources inconnues » dans les réglages développeur d'Android Auto (voir [`android-voiture.md`](android-voiture.md)).

## Déclenchement par bouton

Estompe peut être choisie comme **assistant numérique** du téléphone (réglages système › Applications par défaut ; bouton dans les réglages de l'assistant).
Le geste d'assistant du système (appui long sur le bouton d'alimentation ou d'accueil, balayage depuis un coin) ou le bouton de commande vocale d'un volant
ou d'un casque Bluetooth ouvre alors `AssistActivity`, une activité transparente qui lance l'écoute d'un seul ordre, après « Oui ? », sans mot d'éveil.

Le micro n'est ouvert que le temps de l'ordre et le service s'arrête ensuite : rien ne tourne entre deux ordres. Cela marche quel que soit le réglage
« Écoute », y compris « Éteint », et après un redémarrage du téléphone, puisque c'est l'activité qui met Estompe au premier plan avant d'ouvrir le micro.
L'activité s'affiche sur l'écran de verrouillage : comme pour le mot d'éveil, la relecture avant envoi est la seule barrière.
L'ordre est écouté par le micro du téléphone, pas par celui de la voiture.

## Ordres

L'ordre se dit d'une traite après le mot d'éveil (« Estompe, lis mes messages »), ou après que l'assistant a répondu « Oui ? » si l'on ne dit que « Estompe ».
Un ordre non compris est redemandé une fois (« Je n'ai pas compris. Répétez ? ») sans redire le mot d'éveil.

Dès que le mot d'éveil est reconnu, un bandeau s'affiche en haut de l'écran, par-dessus l'application en cours : « J'écoute… », puis la phrase
comprise entre guillemets, puis ce que l'assistant répond. Il disparaît à la fin de l'ordre, ou d'un toucher. Sans l'autorisation d'affichage
par-dessus les autres applications, seule la notification « Assistant vocal » suit ces étapes (« J'écoute… », « Entendu : … », « Je réponds… »).
Le bandeau suit le thème clair ou sombre du téléphone et n'apparaît pas sur l'écran de verrouillage.

| Ordre | Exemples |
|---|---|
| Lire les messages non lus | « lis mes messages », « est-ce que j'ai des messages » |
| Lire les messages d'un contact | « lis les messages de Marie », « lis le dernier message de Marie » |
| Répondre au dernier message lu | « réponds » |
| Envoyer un SMS | « envoie un SMS à Marie », « dis à Marie que j'arrive dans dix minutes » |
| Musique | « pause », « reprends », « suivant », « précédent », « plus fort », « moins fort » |
| Ouvrir une application de musique | « lance YouTube Music », « ouvre Spotify » |
| Lancer par recherche | « mets Daft Punk », « joue du jazz sur YouTube Music » |
| Playlist | « lance ma playlist Route » |
| Divers | « aide », « annule » |

- **Lecture** : SMS non lus des sept derniers jours et notifications de messagerie encore présentes des applications cochées, les cinq plus récents
  par demande. Chaque annonce dit la provenance et la date : « SMS de Marie, hier à 14 h 30 : … », « Message de Paul sur WhatsApp, il y a 5 minutes : … ».
  « Lis les messages de Marie » lit ses cinq derniers SMS reçus (pas ceux qui lui ont été envoyés) et ses notifications en attente ;
  « lis le dernier message de Marie » n'en lit qu'un. Un SMS lu à la voix n'est pas relu
  (Estompe le mémorise ; seule l'app SMS par défaut peut le marquer comme lu). MMS et conversations de groupe sont ignorés.
- **Envoi** : le destinataire est cherché dans le carnet ; s'il y a plusieurs contacts ou plusieurs numéros, l'assistant les énonce et attend le choix.
  Le destinataire et le texte sont toujours relus, et rien ne part sans le mot « envoyer » (« oui » ne suffit pas ; « annuler » et « recommencer » sont aussi compris).
  Ce comportement ne dépend pas du réglage « Confirmer avant d'envoyer » du mode conduite.
- **Autres messageries** (Signal, WhatsApp…) : lecture de la notification et réponse par son action « Répondre » seulement ; pas d'envoi à l'initiative
  de l'utilisateur. Les messages RCS suivent cette règle : ils ne sont pas dans la base SMS, il faut cocher l'application de SMS dans « Applications lues ».
- **Mémoire des messages** : les messages des applications cochées sont notés à l'arrivée de leur notification (`MessageLog`, fichier privé de l'application,
  300 messages et 30 jours au plus) pour que « lis le dernier message de Marie » retrouve un RCS ou un message WhatsApp dont la notification a disparu.
  Seuls les messages arrivés après l'activation y figurent ; la mémoire est effacée quand l'écoute est éteinte ou qu'aucune application n'est cochée.
  Un message retrouvé dans cette mémoire et venu de l'application de SMS reçoit sa réponse par SMS.
- **Musique** : le transport agit sur la session média active. La recherche et les playlists dépendent de ce que l'application accepte ;
  quand elle n'ouvre pas son catalogue, la playlist est lancée par une recherche sur son nom.
  Le nom de l'application est rapproché même mal transcrit (« YouTube Musique », « you tube music », « YT Music »).
  Une application fermée, ou qui refuse son catalogue aux autres applications (YouTube Music), est ouverte à l'écran : Android ne le permet depuis
  l'arrière-plan que si Estompe a l'autorisation d'affichage par-dessus les autres applications.

## Reconnaissance

Tout passe par le modèle Vosk français du mode conduite, utilisé de trois façons (`Hearing.kt`) :

- **Veille** : le reconnaisseur est restreint à une grammaire, le mot « estompe » et quelques centaines de mots courants qui servent de leurres.
  Il n'a qu'à distinguer le mot d'éveil du reste, ce qui coûte environ dix fois moins de calcul que la dictée libre et le rate moins souvent.
  Le son des quinze dernières secondes est gardé en mémoire vive, jamais écrit sur le disque.
- **Ordre** : ce qui suit le mot d'éveil (repris dans cette mémoire) ou la question « Oui ? » est décodé deux fois. D'abord contre la liste des ordres connus
  (`commandPhrases` : ordres fixes, « lis les messages de… » et « envoie un SMS à… » pour chaque contact, « lance… » pour chaque application de musique) ;
  si la phrase reconnue est exactement l'une d'elles, elle est retenue. Sinon la dictée libre s'applique, comme pour un titre ou un artiste.
  Les confirmations (« envoyer », « annuler »…) et les choix (« un », « deux »…) suivent le même principe.
- **Dictée** du texte d'un SMS : dictée libre seule.

Quand rien n'est compris, l'analyse réessaie en corrigeant les homophones courants du premier mot (« mais » pour « mets », « l'ancien » pour « lance »,
« posait » pour « pause »…).

Un nom de contact ou d'application absent du vocabulaire du modèle n'entre pas dans la grammaire : il reste reconnu, moins bien, par la dictée libre.

### Diagnostic (version de test seulement)

Dans un APK debug, le journal (`adb logcat -s EstompeDrive`) donne pour chaque ordre la phrase retenue, le résultat de la grammaire et celui de la dictée libre.
Le réglage « Enregistrer le son des ordres », absent d'une version de production, garde en plus ces extraits en WAV dans le dossier de l'application
(`adb pull /sdcard/Android/data/fr.plbls.estompe/files/assistant`), 60 fichiers au plus, pour mesurer la reconnaissance sur une vraie voix.

## Écran verrouillé

L'assistant répond écran verrouillé : toute personne à portée de voix peut faire lire les messages ou dicter un SMS. La relecture avant envoi est la seule barrière.

## IA locale (facultative)

Quand aucun motif ne reconnaît la phrase, un modèle de langage embarqué (llama.cpp, liaison `io.github.ljcamargo:llamacpp-kotlin`) peut proposer
une intention parmi la même liste fermée. Sa sortie est contrainte par une grammaire puis validée : une réponse hors format ne déclenche rien, et un SMS
proposé par le modèle passe par la même relecture. Le modèle (Qwen 2.5 0.5B, quelques centaines de Mo) se télécharge et se supprime depuis les réglages ;
il est chargé à la demande et libéré après deux minutes sans usage. Sans lui, l'assistant répond « Je n'ai pas compris. ».

## Code

| Fichier (`apps/estompe/android/src-tauri/gen/android/app/src/main/java/fr/plbls/estompe/`) | Rôle |
|---|---|
| `Assistant.kt` | logique pure : mot d'éveil, motifs des ordres, rapprochement des contacts, annonce des messages |
| `AssistantService.kt` | service au premier plan : veille du mot d'éveil, dialogues, réglages (`AssistantStore`) |
| `AssistantOverlay.kt` | bandeau affiché par-dessus les autres applications |
| `CarService.kt` | entrée « média » d'Android Auto : actions et état sur l'écran de la voiture |
| `AssistActivity.kt` | déclenchement par le geste d'assistant du système ou le bouton vocal Bluetooth |
| `Voice.kt` | synthèse, écoute Vosk, focus audio, arbitre du micro ; partagé avec `DriveService` |
| `Hearing.kt` | détection du mot d'éveil par grammaire, décodage d'un ordre en deux passes |
| `Messages.kt` | SMS et contacts (`Phone`) |
| `AssistantListener.kt` | notifications des autres messageries, sessions média |
| `Music.kt` | pilotage des apps de musique (`MusicControl`) |
| `Llm.kt` | modèle de langage local : téléchargement, consigne, validation de la sortie |

Réglages côté front : `apps/estompe/android/src/SettingsAssistant.svelte` et `assistant.ts`. L'état se lit par les méthodes `assistant*` du pont `EstompeNative` ;
tout ce qui modifie un réglage ou liste les applications installées passe par l'opération `assistant` du canal `EstompeSession`, réservé au cadre principal
(une iframe tierce ne peut pas activer l'écoute).
Tests : `AssistantTest.kt`, `AssistantServiceTest.kt`, `LlmTest.kt` (JUnit) et `assistant.test.ts` (vitest).
