# Assistant vocal d'Estompe (Android)

Un assistant réveillé par le mot « Estompe », qui lit les messages, envoie un SMS et pilote la musique, sans toucher le téléphone.
La reconnaissance, la compréhension et la synthèse restent sur l'appareil : aucun service Google, aucune donnée vocale envoyée sur le réseau.
Il complète le [mode conduite](android-voiture.md), dont il réutilise la synthèse vocale et la dictée Vosk.

## Activer

Paramètres › Notifications › **Assistant vocal** :

| Réglage | Effet |
|---|---|
| Écoute | « Éteint » (par défaut), « En mode conduite seulement », « Tout le temps » |
| Applications lues | messageries dont les notifications sont lues, en plus des SMS |
| Application de musique par défaut | celle qui reçoit « mets… » quand aucune app n'est nommée ; sinon la dernière utilisée |
| Compréhension étendue (IA locale) | modèle de langage facultatif, utilisé seulement quand aucun ordre n'est reconnu |

Autorisations, demandées depuis ce même écran :

- **Micro** et **notifications** : nécessaires à l'écoute ;
- **SMS** et **contacts** : lecture et envoi de SMS, noms du carnet ;
- **Accès aux notifications** (réglage système) : lecture des autres messageries et pilotage de la musique.

Le modèle de dictée français (~40 Mo) est celui du mode conduite : il doit être téléchargé.

Tant que l'écoute est active, une notification permanente « Assistant vocal » le signale et l'indicateur de micro d'Android reste affiché.
Android interdit de rouvrir le micro depuis l'arrière-plan : après un redémarrage du téléphone, ou quand le mode conduite démarre seul
à la connexion Bluetooth, la notification « Assistant vocal en pause » demande d'ouvrir Estompe pour relancer l'écoute.

## Ordres

L'ordre se dit d'une traite après le mot d'éveil (« Estompe, lis mes messages »), ou après le signal sonore si l'on ne dit que « Estompe ».

| Ordre | Exemples |
|---|---|
| Lire les messages non lus | « lis mes messages », « est-ce que j'ai des messages » |
| Lire les messages d'un contact | « lis les messages de Marie » |
| Répondre au dernier message lu | « réponds » |
| Envoyer un SMS | « envoie un SMS à Marie », « dis à Marie que j'arrive dans dix minutes » |
| Musique | « pause », « reprends », « suivant », « précédent », « plus fort », « moins fort » |
| Lancer par recherche | « mets Daft Punk », « joue du jazz sur YouTube Music » |
| Playlist | « lance ma playlist Route » |
| Divers | « aide », « annule » |

- **Lecture** : SMS non lus, puis notifications de messagerie encore présentes des applications cochées. Un SMS lu à la voix n'est pas relu
  (Estompe le mémorise ; seule l'app SMS par défaut peut le marquer comme lu). MMS et conversations de groupe sont ignorés.
- **Envoi** : le destinataire est cherché dans le carnet ; s'il y a plusieurs contacts ou plusieurs numéros, l'assistant les énonce et attend le choix.
  Le destinataire et le texte sont toujours relus, et rien ne part sans « envoyer » (« annuler » et « recommencer » sont aussi compris).
  Ce comportement ne dépend pas du réglage « Confirmer avant d'envoyer » du mode conduite.
- **Autres messageries** (Signal, WhatsApp…) : lecture de la notification et réponse par son action « Répondre » seulement ; pas d'envoi à l'initiative
  de l'utilisateur. Les messages RCS suivent cette règle : ils ne sont pas dans la base SMS.
- **Musique** : le transport agit sur la session média active. La recherche et les playlists dépendent de ce que l'application accepte ;
  quand elle n'ouvre pas son catalogue, la playlist est lancée par une recherche sur son nom.

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
| `Voice.kt` | synthèse, écoute Vosk, focus audio, arbitre du micro ; partagé avec `DriveService` |
| `Messages.kt` | SMS et contacts (`Phone`) |
| `AssistantListener.kt` | notifications des autres messageries, sessions média |
| `Music.kt` | pilotage des apps de musique (`MusicControl`) |
| `Llm.kt` | modèle de langage local : téléchargement, consigne, validation de la sortie |

Réglages côté front : `apps/estompe/android/src/SettingsAssistant.svelte` et `assistant.ts`, par les méthodes `assistant*` du pont `EstompeNative`.
Tests : `AssistantTest.kt`, `AssistantServiceTest.kt`, `LlmTest.kt` (JUnit) et `assistant.test.ts` (vitest).
