# Gateway et propagation de l'identité

Les micro-services ne voient jamais de token Keycloak : le gateway authentifie, contrôle les rôles, puis injecte des en-têtes d'identité signés.

## Routage

Déclaré dans `config/default.toml`, un bloc `[[routes]]` par micro-service :

| Champ | Rôle |
| --- | --- |
| `prefix` | Chemin public, ex. `/api/chat` |
| `upstream` | URL interne, ex. `http://chat-service:8080` |
| `strip_prefix` | Retire le préfixe avant de transmettre |
| `public` | `true` = pas d'auth exigée (identité injectée si présente ; Redis ou Keycloak indisponible : requête transmise en anonyme) |
| `roles` | Au moins un de ces rôles requis (realm ou `client:role`), sinon 403 |
| `forward_token` | Transmet aussi l'access token en `Authorization` (cas rares) |

## En-têtes injectés

Toujours supprimés de la requête entrante avant injection, pour empêcher l'usurpation :

| En-tête | Contenu |
| --- | --- |
| `X-User-Id` | `sub` Keycloak |
| `X-User-Name` | `preferred_username`, en percent-encoding UTF-8 (`Jérôme` → `J%C3%A9r%C3%B4me`) |
| `X-User-Email` | `email`, même encodage (`%`, espace, caractères de contrôle et non ASCII encodés) |
| `X-User-Roles` | Rôles realm tels quels + rôles client sous la forme `client:role`, séparés par des virgules |
| `X-Client-Id` | `azp` : quel client a ouvert la session |
| `X-Identity-Ts` | Horodatage Unix de l'injection |
| `X-Identity-Sig` | HMAC-SHA256 (base64url) de `id\nroles\nclient\nts\nname\nemail` avec le secret interne (`GATEWAY_IDENTITE_SECRET`), `name` et `email` tels qu'envoyés (encodés) |
| `X-Request-Id` | Identifiant de corrélation, aussi renvoyé au client (celui de l'upstream est remplacé) |

Côté micro-service, on vérifie la signature et que `ts` a moins de 60 s, puis on décode `X-User-Name` et `X-User-Email` (percent-decoding UTF-8). Un en-tête absent compte comme une valeur vide dans la signature. Vérificateurs : aegis et permission. La passerelle et ces services se déploient ensemble : un ancien vérificateur refuse les nouvelles signatures, et inversement.

## Réponses d'erreur

401 sans identité valide sur une route privée, 403 si rôle manquant ou `Origin` refusé, 502 si l'upstream ne répond pas.
