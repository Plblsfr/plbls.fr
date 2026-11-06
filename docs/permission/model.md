# Modèle de permissions (RBAC par application)

```
application ──< permission
     └────────< rôle ──< (rôle, permission)
                  └────< attribution (compte, rôle)
```

| Objet | Rôle | Exemple |
| --- | --- | --- |
| **Application** | Un micro-service ou un site de plbls.fr. Clé `[a-z][a-z0-9-]*`, 40 car. max | `chat` |
| **Permission** | Une action précise, déclarée par application. Nom complet `application:permission` | `chat:message.delete` |
| **Rôle** | Un paquet de permissions **de la même application** (garanti par la base) | `chat/moderator` = `message.delete` + `user.mute` |
| **Attribution** | Un rôle donné à un compte. Le compte = le `sub` Keycloak (`X-User-Id`) | `alice` → `chat/moderator` |

Règles :

- Une permission d'un compte = l'union des permissions de tous ses rôles. Il n'y a **pas de refus explicite** : tout ce qui n'est pas accordé est interdit.
- Pas de permission donnée directement à un compte : toujours via un rôle (créer un rôle d'une seule permission si besoin).
- Supprimer une permission la retire de tous les rôles ; supprimer un rôle supprime ses attributions ; supprimer une application supprime tout ce qui en dépend.
- Permissions et rôles sont **déclarés dans permission** (UI ou API), pas dans le code des services : un service n'a besoin que de connaître les noms qu'il teste.
- Les rôles Keycloak (`X-User-Roles`) ne servent qu'à l'accès grossier au gateway et à l'administration de permission (`permission-admin`). Ils ne sont pas copiés ici.
- Chaque modification est écrite dans le journal d'audit (`GET /v1/audit`).

## Qui administre permission

Permission gère son propre accès avec son propre modèle. Est administrateur :

1. un compte portant le **rôle Keycloak** `permission-admin` (réglable : `server.admin_role`) : le point de départ, et la porte de secours qui ne dépend pas de la base ;
2. un compte qui a la **permission `permission:admin`**, c'est-à-dire un rôle de l'application réservée `permission`. Le rôle `permission/admin` est créé par la migration ; on l'attribue depuis l'interface comme n'importe quel autre.

L'application `permission` ne peut pas être supprimée. Un token de service n'est jamais administrateur. Un administrateur peut en nommer d'autres : on ne peut pas se verrouiller dehors tant que le rôle Keycloak existe.

## Recherche de comptes par e-mail

Dans l'écran Attributions, choisir un compte par début d'e-mail demande la permission **`permission:users.read`** (rôle `permission/users-reader`), en plus d'être administrateur. Sans elle, l'interface garde la saisie du `sub`. Le backend relaie `GET /v1/users?email=` vers la route interne `/internal/users` du gateway (`PERMISSION__GATEWAY__URL`, `PERMISSION__GATEWAY__INTERNAL_TOKEN`) ; non configuré, la route répond 404.

## Évolutions possibles (non faites)

Conditions sur les ressources (ex. « seulement ses propres messages »), expiration d'une attribution, groupes de comptes. Le modèle actuel ne les empêche pas : une table de plus, pas une refonte.
