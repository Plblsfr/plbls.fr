#!/usr/bin/env python3
"""Test de bout en bout contre un backend lancé (make dev-up dev-run). Signe les en-têtes comme auth-gateway."""
import json
import os
import sys
import time

from devlib import BASE, call, req, sign

SVC = os.environ.get("SERVICE_TOKEN", "dev-chat-token")
RUN = str(int(time.time()))[-6:]  # relançable sans nettoyage
APP = f"smoke{RUN}"

fails = 0
def expect(label, got, want):
    global fails
    ok = got == want
    fails += not ok
    print(("ok   " if ok else "FAIL ") + label + ("" if ok else f"  (obtenu {got!r}, attendu {want!r})"))

admin = sign("admin-1", "user,permission-admin")
alice = sign("alice", "user")
svc = {"Authorization": f"Bearer {SVC}"}

expect("sans identité -> 401", call("GET", "/v1/me/permissions", {})[0], 401)
expect("signature falsifiée -> 401", call("GET", "/v1/me/permissions", {**alice, "X-User-Roles": "user,permission-admin"})[0], 401)
expect("non-admin sur /v1/apps -> 403", call("GET", "/v1/apps", alice)[0], 403)
expect("service non admin -> 403", call("GET", "/v1/apps", svc)[0], 403)
expect("token service inconnu -> 401", call("GET", "/v1/users/alice/permissions", {"Authorization": "Bearer nope"})[0], 401)

expect("création app", call("POST", "/v1/apps", admin, {"key": APP, "name": "Smoke"})[0], 201)
expect("app en double -> 409", call("POST", "/v1/apps", admin, {"key": APP, "name": "Smoke"})[0], 409)
expect("clé invalide -> 400", call("POST", "/v1/apps", admin, {"key": "Bad Key", "name": "x"})[0], 400)
for k in ("message.read", "message.delete"):
    expect(f"permission {k}", call("POST", f"/v1/apps/{APP}/permissions", admin, {"key": k})[0], 201)
st, role = call("POST", f"/v1/apps/{APP}/roles", admin, {"key": "moderator", "name": "Modérateur", "permissions": ["message.delete"]})
expect("rôle créé", (st, role["permissions"]), (201, ["message.delete"]))
expect("rôle avec permission inconnue -> 400", call("POST", f"/v1/apps/{APP}/roles", admin, {"key": "x", "name": "x", "permissions": ["nope"]})[0], 400)
expect("rôle raté annulé (transaction)", [r["key"] for r in call("GET", f"/v1/apps/{APP}/roles", admin)[1]], ["moderator"])

expect("alice n'a rien", call("GET", f"/v1/me/permissions?app={APP}", alice)[1]["permissions"], [])
expect("attribution", call("POST", "/v1/assignments", admin, {"user_id": "alice", "app": APP, "role": "moderator"})[0], 204)
expect("attribution idempotente", call("POST", "/v1/assignments", admin, {"user_id": "alice", "app": APP, "role": "moderator"})[0], 204)
expect("alice a message.delete", call("GET", f"/v1/me/permissions?app={APP}", alice)[1]["permissions"], [f"{APP}:message.delete"])
expect("check alice (soi-même)", call("POST", "/v1/check", alice, {"permission": f"{APP}:message.delete"})[1], {"allowed": True})
expect("check alice autre perm", call("POST", "/v1/check", alice, {"permission": f"{APP}:message.read"})[1], {"allowed": False})
expect("check par service", call("POST", "/v1/check", svc, {"user_id": "alice", "permission": f"{APP}:message.delete"})[1], {"allowed": True})
expect("service sans user_id -> 400", call("POST", "/v1/check", svc, {"permission": f"{APP}:message.delete"})[0], 400)
expect("alice lit bob -> 403", call("GET", "/v1/users/bob/permissions", alice)[0], 403)
expect("service lit alice", call("GET", "/v1/users/alice/permissions", svc)[0], 200)
expect("format permission invalide -> 400", call("POST", "/v1/check", alice, {"permission": "sansdeuxpoints"})[0], 400)

expect("PUT rôle remplace les permissions", call("PUT", f"/v1/apps/{APP}/roles/moderator", admin, {"name": "Modo", "permissions": ["message.read"]})[1]["permissions"], ["message.read"])
expect("alice a perdu message.delete", call("POST", "/v1/check", alice, {"permission": f"{APP}:message.delete"})[1], {"allowed": False})
expect("suppression d'une permission la retire des rôles", call("DELETE", f"/v1/apps/{APP}/permissions/message.read", admin)[0], 204)
expect("rôle vidé", call("GET", f"/v1/apps/{APP}/roles", admin)[1][0]["permissions"], [])
expect("révocation", call("DELETE", f"/v1/assignments?user_id=alice&app={APP}&role=moderator", admin)[0], 204)
expect("révocation absente -> 404", call("DELETE", f"/v1/assignments?user_id=alice&app={APP}&role=moderator", admin)[0], 404)
audit = call("GET", "/v1/audit?limit=200", admin)[1]
expect("audit contient les actions", {"app.create", "role.create", "assignment.grant", "assignment.revoke", "permission.delete"} <= {a["action"] for a in audit}, True)
expect("suppression app en cascade", call("DELETE", f"/v1/apps/{APP}", admin)[0], 204)
expect("alice n'a plus rien", call("GET", "/v1/me/permissions", alice)[1]["permissions"], [])

# Accès à permission lui-même : permission:admin (application réservée « permission »)
bob = sign(f"bob{RUN}", "user")
expect("bob (rôle user) n'administre pas", call("GET", "/v1/apps", bob)[0], 403)
expect("l'application réservée existe", "permission" in [a["key"] for a in call("GET", "/v1/apps", admin)[1]], True)
expect("attribution de permission/admin à bob", call("POST", "/v1/assignments", admin, {"user_id": f"bob{RUN}", "app": "permission", "role": "admin"})[0], 204)
expect("bob administre maintenant", call("GET", "/v1/apps", bob)[0], 200)
expect("check permission:admin pour bob", call("POST", "/v1/check", bob, {"permission": "permission:admin"})[1], {"allowed": True})
expect("bob peut lire les permissions d'un autre (admin)", call("GET", "/v1/users/alice/permissions", bob)[0], 200)
expect("le service reste non admin même avec ce nom", call("GET", "/v1/apps", svc)[0], 403)
expect("l'application réservée ne se supprime pas", call("DELETE", "/v1/apps/permission", admin)[0], 400)
expect("révocation de permission/admin", call("DELETE", f"/v1/assignments?user_id=bob{RUN}&app=permission&role=admin", admin)[0], 204)
expect("bob n'administre plus", call("GET", "/v1/apps", bob)[0], 403)

# MCP
def mcp(token, msg):
    return req(BASE + "/mcp", "POST", {"Authorization": f"Bearer {token}"}, json.dumps(msg).encode())[0]
expect("MCP sans bon token -> 401", mcp("x", {"id": 1, "method": "ping"}), 401)
expect("MCP avec token", mcp(os.environ.get("PERMISSION__MCP__TOKEN", "dev-mcp-token"), {"jsonrpc": "2.0", "id": 1, "method": "tools/list"}), 200)

print("\n" + ("TOUT EST OK" if not fails else f"{fails} ÉCHEC(S)"))
sys.exit(1 if fails else 0)
