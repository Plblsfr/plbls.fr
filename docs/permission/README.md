# permission — documentation

Remplacé par [aegis](../aegis/README.md) : ce service reste en place jusqu'à la bascule d'Estompe et de pli (voir [aegis › intégration](../aegis/integration.md)), puis sera retiré.

`permission` (**permission.plbls.fr**) décide ce que chaque compte plbls.fr a le droit de faire dans chaque application. Les comptes et l'authentification restent dans [security.plbls.fr](https://security.plbls.fr) (auth-gateway + Keycloak) : permission ne gère ni mots de passe ni sessions.

| Document | Contenu |
| --- | --- |
| [model.md](model.md) | Modèle RBAC : applications, permissions, rôles, attributions |
| [integration.md](integration.md) | Brancher un micro-service ou un frontend, exemples |
| [architecture.md](architecture.md) | Stack, authentification, sécurité, décisions |

Spec OpenAPI : `GET /openapi.json`.
