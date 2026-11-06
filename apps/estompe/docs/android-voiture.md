# Estompe en voiture (Android)

Deux façons de lire ses messages et d'y répondre à la voix, sans toucher le téléphone.

## 1. Android Auto et surcouches (notifications)

Les messages privés et mentions arrivent comme des conversations (`MessagingStyle`) avec « Répondre » et « Marquer comme lu ».
Android Auto (ou une surcouche comme Gearslip) lit le message et envoie la réponse dictée ; Estompe n'assure ni la lecture ni la dictée.

- L'APK n'est pas publié sur le Play Store : dans Android Auto, activer le **mode développeur** (toucher 10 fois la version dans
  ses paramètres) puis **Sources inconnues**, sinon l'application n'apparaît pas.
- Sur **GrapheneOS**, la lecture vocale d'Android Auto exige Google Assistant : utiliser le mode conduite ci-dessous ou une
  surcouche qui lit les notifications standard.
- Les réglages de notification d'Estompe s'appliquent (par serveur, par type, ne pas déranger). Aperçu masqué : seul
  « Nouveau message de X » est lu.
- Test : émulateur Android Auto « Desktop Head Unit » (voir `docs/android-emulateur.md`).

## 2. Mode conduite d'Estompe (sans Google)

Paramètres › Notifications › **Mode conduite**, ou tuile « Mode conduite » des réglages rapides.

- Lit « Message de X dans Y : contenu » pour les messages privés et mentions (synthèse vocale installée sur le téléphone :
  RHVoice, eSpeak…), baisse la musique, lit les messages simultanés à la suite.
- **Répondre** : bouton « Répondre » de la notification du mode, ou lecture/pause du volant dans les 20 s après une lecture.
  Dicter, écouter la relecture, dire « envoyer », « annuler » ou « recommencer » (envoi automatique si la confirmation est désactivée).
- Dictée avec Vosk, hors ligne : le modèle français (~40 Mo) se télécharge une fois (bouton dans les réglages). Aucune donnée
  vocale ne quitte le téléphone ; le micro de la voiture est utilisé via Bluetooth quand il est disponible.
- Démarrage automatique à la connexion d'un appareil Bluetooth choisi (la voiture).
- Permissions demandées : micro, notifications, Bluetooth (si démarrage automatique).
