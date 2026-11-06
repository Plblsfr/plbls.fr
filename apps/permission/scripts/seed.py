#!/usr/bin/env python3
"""Données de départ pour le dev : applications chat et radar avec permissions, rôles et quelques attributions. Idempotent."""
from devlib import call, sign

admin = sign("dev-admin", "user,permission-admin")

def put(method, path, body):
    st, res = call(method, path, admin, body)
    print(("ok   " if st in (200, 201, 204) else "déjà " if st == 409 else f"ERR {st} ") + f"{method} {path}")
    return st

APPS = {
    "chat": ("Chat", {
        "message.read": "Lire les messages",
        "message.post": "Écrire un message",
        "message.delete": "Supprimer le message d'un autre",
        "user.mute": "Rendre muet un utilisateur",
        "user.ban": "Bannir un utilisateur",
    }, {
        "member": ("Membre", ["message.read", "message.post"]),
        "moderator": ("Modérateur", ["message.read", "message.post", "message.delete", "user.mute"]),
        "admin": ("Administrateur", ["message.read", "message.post", "message.delete", "user.mute", "user.ban"]),
    }),
    "radar": ("Radar immo", {
        "alert.read": "Voir les alertes",
        "alert.create": "Créer une alerte",
        "export": "Exporter les annonces",
    }, {
        "viewer": ("Lecteur", ["alert.read"]),
        "editor": ("Éditeur", ["alert.read", "alert.create", "export"]),
    }),
}

for key, (name, perms, roles) in APPS.items():
    put("POST", "/v1/apps", {"key": key, "name": name})
    for p, desc in perms.items():
        put("POST", f"/v1/apps/{key}/permissions", {"key": p, "description": desc})
    for r, (rname, rperms) in roles.items():
        if put("POST", f"/v1/apps/{key}/roles", {"key": r, "name": rname, "permissions": rperms}) == 409:
            put("PUT", f"/v1/apps/{key}/roles/{r}", {"name": rname, "permissions": rperms})

for user, app, role in [("a1b2c3d4-0000-4000-8000-000000000001", "chat", "moderator"),
                        ("a1b2c3d4-0000-4000-8000-000000000002", "chat", "member"),
                        ("a1b2c3d4-0000-4000-8000-000000000002", "radar", "viewer")]:
    put("POST", "/v1/assignments", {"user_id": user, "app": app, "role": role})
