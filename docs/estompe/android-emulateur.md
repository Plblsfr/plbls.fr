# Tester les notifications Android sur émulateur

WSL n'expose pas la virtualisation (`/dev/kvm` absent) : l'émulateur tourne **sous Windows**, l'APK se construit dans WSL
(ou directement sous Windows). Procédure non exécutée de bout en bout : à valider au premier usage.

## 1. Émulateur (Windows, une fois)

1. Installer Android Studio, activer la virtualisation dans le BIOS (Hyper-V / « Windows Hypervisor Platform » activés).
2. Device Manager › Create Device › Pixel récent › image système **x86_64 « Google Play »** (API 34 ou plus). Les images
   « Google APIs » sans Play Store n'apportent pas FCM de façon fiable.
3. Démarrer l'AVD, ouvrir le Play Store et s'y connecter avec un compte Google (FCM exige les Google Play Services connectés).

## 2. Clés FCM (action manuelle)

Voir `web/estompe/README.md` (« Push via FCM ») : projet Firebase, app Android `fr.plbls.estompe`,
`apps/estompe/android/src-tauri/gen/android/app/google-services.json` (ignoré par git) côté app, `FCM_SERVICE_ACCOUNT` côté API
(variable commune, `deploy/.env.example`). Sans `google-services.json`, l'APK se construit mais sans push.

## 3. Build debug

L'émulateur Windows est en x86_64 (le script de release ne cible que arm64) :

```sh
rustup target add x86_64-linux-android
cd apps/estompe/android
PATH=../../../node_modules/.bin:$PATH tauri android build --debug --apk --target x86_64
# → apps/estompe/android/src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

Prérequis : JDK 17, SDK Android, NDK 26, `ANDROID_HOME`, `NDK_HOME` (voir `web/estompe/README.md`).

## 4. Installation avec adb

Depuis Windows (`adb` des platform-tools) :

```powershell
adb devices
adb install -r app-universal-debug.apk
```

Depuis WSL, joindre l'adb Windows : lancer `adb -a nodaemon server start` côté Windows, puis dans WSL
`ADB_SERVER_SOCKET=tcp:<IP-hôte-Windows>:5037 adb install -r app-universal-debug.apk` (IP : `ip route show default`).
Le plus simple reste de copier l'APK sur Windows (`/mnt/c/...`).

L'API est joignable depuis l'émulateur à `10.0.2.2` (hôte) ; pour l'instance de production, rien à changer.

## 5. Tester le push app fermée

1. Se connecter dans l'app, Paramètres › « Notifications push » : accepter la permission. Le jeton FCM est enregistré
   (`push_subscriptions`, type `fcm`).
2. Fermer l'app pour de bon : `adb shell am force-stop fr.plbls.estompe` (ne pas passer par « Forcer l'arrêt » dans les
   réglages Android : il bloque FCM jusqu'au prochain lancement).
3. Depuis un autre compte (ou le web), envoyer un message privé ou une mention : la notification doit apparaître.
4. Diagnostic : `adb logcat -s FirebaseMessaging FCM` côté appareil ; journaux de l'API (`notify`, échecs `fcm`) côté serveur.
   Un jeton refusé en 404/410 est retiré automatiquement.
