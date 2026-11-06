#!/bin/sh
# Génère au démarrage la configuration d'exécution (config.js) et la politique de sécurité (CSP) d'après l'environnement.
#   API_URL           URL publique de l'API Estompe, ex. https://chat.plbls.fr
#   GATEWAY_URL       URL d'auth-gateway, ex. https://security.plbls.fr
#   VOICE_URL         URL publique de LiveKit (wss://...), ex. wss://voice.plbls.fr
#   KEYCLOAK_ISSUER   (facultatif) émetteur Keycloak, utile seulement à l'app Windows
set -eu

: "${API_URL:?API_URL est obligatoire}"
GATEWAY_URL="${GATEWAY_URL:-https://security.plbls.fr}"
VOICE_URL="${VOICE_URL:-}"
KEYCLOAK_ISSUER="${KEYCLOAK_ISSUER:-}"

# Les valeurs finissent dans du JavaScript et dans un en-tête HTTP : on refuse tout caractère inattendu.
for v in "$API_URL" "$GATEWAY_URL" "$VOICE_URL" "$KEYCLOAK_ISSUER"; do
  case "$v" in
    *[!A-Za-z0-9:/._-]*) echo "valeur de configuration invalide : $v" >&2; exit 1 ;;
  esac
done

API_WS=$(printf '%s' "$API_URL" | sed 's#^http#ws#')
# LiveKit : signalisation en wss, repli de validation en https
VOICE_HTTP=$(printf '%s' "$VOICE_URL" | sed 's#^ws#http#')

cat > /usr/share/nginx/html/config.js <<JS
globalThis.__ESTOMPE__ = { apiUrl: "$API_URL", gatewayUrl: "$GATEWAY_URL", keycloakIssuer: "$KEYCLOAK_ISSUER" }
JS

CSP="default-src 'self'; script-src 'self' 'wasm-unsafe-eval' https://www.youtube.com https://player.twitch.tv https://player.vimeo.com; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: $API_URL https://*.giphy.com https://img.youtube.com; font-src 'self'; media-src 'self' blob: $API_URL https:; connect-src 'self' https: $API_URL $API_WS $VOICE_URL $VOICE_HTTP; worker-src 'self'; manifest-src 'self'; object-src 'none'; frame-src https://www.youtube-nocookie.com https://player.twitch.tv https://clips.twitch.tv https://player.vimeo.com https://www.dailymotion.com; base-uri 'none'; frame-ancestors 'none'; form-action 'self' $GATEWAY_URL"

cat > /etc/nginx/estompe-headers.inc <<HDR
add_header Content-Security-Policy "$CSP" always;
add_header X-Content-Type-Options "nosniff" always;
add_header X-Frame-Options "DENY" always;
add_header Referrer-Policy "strict-origin-when-cross-origin" always;
add_header Permissions-Policy "camera=(self), display-capture=(self), geolocation=(), microphone=(self)" always;
HDR
