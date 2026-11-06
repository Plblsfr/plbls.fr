#!/usr/bin/env python3
"""Refresh de session : access token expiré en Redis -> refresh transparent, et un seul refresh sous concurrence
(le realm révoque les refresh tokens : deux refresh simultanés feraient échouer le second)."""
import json
import subprocess
from concurrent.futures import ThreadPoolExecutor

from devlib import GW, Checks, login, redis, req, session_key

c = Checks()
sid = login()
key = session_key(sid)
cookie = {"Cookie": f"plbls_sid={sid}"}


def stored():
    return json.loads(redis("GET", key))


def expire_access():
    s = stored()
    s["access_exp"] = 0
    redis("SET", key, json.dumps(s), "KEEPTTL")
    return s


print("refresh simple")
before = expire_access()
status, _, _ = req(f"{GW}/auth/me", headers=cookie)
after = stored()
c.check("/auth/me répond 200", status == 200, status)
c.check("access token renouvelé", after["access_token"] != before["access_token"] and after["access_exp"] > 0)
c.check("refresh token tourné", after["refresh_token"] != before["refresh_token"])

print("refresh concurrent (20 appels)")
before = expire_access()
with ThreadPoolExecutor(20) as ex:
    statuses = list(ex.map(lambda _: req(f"{GW}/auth/me", headers=cookie)[0], range(20)))
c.check("tous les appels en 200", set(statuses) == {200}, sorted(set(statuses)))
c.check("session toujours valide", req(f"{GW}/auth/session", headers=cookie)[0] == 204)

print("Keycloak injoignable : la session doit survivre")
expire_access()
subprocess.run(["docker", "pause", "security-dev-keycloak-1"], check=True, capture_output=True)
try:
    status = req(f"{GW}/auth/me", headers=cookie)[0]
finally:
    subprocess.run(["docker", "unpause", "security-dev-keycloak-1"], check=True, capture_output=True)
c.check("503 (et non 401)", status == 503, status)
c.check("session conservée dans Redis", redis("EXISTS", key) == "1")
c.check("Keycloak revenu : refresh réussi", req(f"{GW}/auth/me", headers=cookie)[0] == 200)

print("refresh token révoqué")
s = stored()
s["access_exp"] = 0
s["refresh_token"] = "invalide"
redis("SET", key, json.dumps(s), "KEEPTTL")
c.check("401", req(f"{GW}/auth/me", headers=cookie)[0] == 401)
c.check("session supprimée de Redis", redis("EXISTS", key) == "0")
c.done()
