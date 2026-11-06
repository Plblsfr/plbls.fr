# Gateway et propagation de l'identité

Les micro-services ne voient jamais de token Keycloak : le gateway authentifie, contrôle les rôles, puis injecte des en-têtes d'identité signés.

## Routage

Déclaré dans `config/default.toml`, un bloc `[[routes]]` par micro-service :

| Champ | Rôle |
| --- | --- |
| `prefix` | Chemin public, ex. `/api/chat` |
| `upstream` | URL interne, ex. `http://chat-service:8080` |
| `strip_prefix` | Retire le préfixe avant de transmettre |
| `public` | `true` = pas d'auth exigée (identité injectée si présente) |
| `roles` | Au moins un de ces rôles requis (realm ou `client:role`), sinon 403 |
| `forward_token` | Transmet aussi l'access token en `Authorization` (cas rares) |

## En-têtes injectés

Toujours supprimés de la requête entrante avant injection, pour empêcher l'usurpation :

| En-tête | Contenu |
| --- | --- |
| `X-User-Id` | `sub` Keycloak |
| `X-User-Name` | `preferred_username` |
| `X-User-Email` | `email` |
| `X-User-Roles` | Rôles realm tels quels + rôles client sous la forme `client:role`, séparés par des virgules |
| `X-Client-Id` | `azp` : quel client a ouvert la session |
| `X-Identity-Ts` | Horodatage Unix de l'injection |
| `X-Identity-Sig` | HMAC-SHA256 (base64url) de `id\nroles\nclient\nts` avec le secret interne |
| `X-Request-Id` | Identifiant de corrélation |

Côté micro-service, on vérifie la signature et que `ts` a moins de 60 s ; une petite lib partagée suffit.

## Réponses d'erreur

401 sans identité valide sur une route privée, 403 si rôle manquant ou `Origin` refusé, 502 si l'upstream ne répond pas.
