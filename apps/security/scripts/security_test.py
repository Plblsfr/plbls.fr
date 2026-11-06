#!/usr/bin/env python3
"""Régressions de sécurité de l'audit : traversée de chemin, login CSRF, logout par simple lien."""
import http.client, re, urllib.parse

from devlib import GW, Checks, login, req

c = Checks()


def raw_get(path):
    """GET avec le chemin tel quel (urllib et Bruno normaliseraient `..`)."""
    conn = http.client.HTTPConnection("127.0.0.1", 8080, timeout=10)
    conn.request("GET", path)
    r = conn.getresponse()
    body = r.read().decode(errors="replace")
    conn.close()
    return r.status, body


print("traversée de chemin dans le proxy")
for path in ["/api/whoami/a/../b/c", "/api/whoami/a/%2e%2e/b/c", "/api/whoami/a/%2E%2E/b", "/api/whoami/a/./b",
             "/api/whoami/a/x%2fy", "/api/whoami/a/x%5cy"]:
    status, _ = raw_get(path)
    c.check(f"400 pour {path}", status == 400, status)
status, body = raw_get("/api/whoami/a/b..c/v1.2")
c.check("chemin normal avec des points : 200", status == 200 and "GET /a/b..c/v1.2 " in body, status)

print("login CSRF")
# l'attaquant fait son propre login et garde l'URL de callback (code + state valides)
_, h, _ = req(f"{GW}/auth/login")
_, h2, body = req(h["Location"])
kc = "; ".join(x.split(";")[0] for x in h2.get_all("Set-Cookie"))
action = re.search(r'action="([^"]+)"', body.decode()).group(1).replace("&amp;", "&")
form = urllib.parse.urlencode({"username": "admin", "password": "admin", "credentialId": ""}).encode()
_, h3, _ = req(action, "POST", {"Cookie": kc, "Content-Type": "application/x-www-form-urlencoded"}, form)
callback = h3["Location"]
status, h4, _ = req(callback)  # la victime ouvre ce lien : aucun cookie de login
cookies = h4.get_all("Set-Cookie") or []
c.check("callback sans cookie de login : 400", status == 400, status)
c.check("aucune session créée", not any(x.startswith("plbls_sid=") for x in cookies))
# le state de l'attaquant n'a pas été consommé par la tentative de la victime
login_cookie = next(x for x in h.get_all("Set-Cookie") if x.startswith("plbls_sid_login=")).split(";")[0]
status, _, _ = req(callback, headers={"Cookie": "plbls_sid_login=autre-valeur"})
c.check("cookie de login d'une autre valeur : 400", status == 400, status)
status, h5, _ = req(callback, headers={"Cookie": login_cookie})
c.check("le vrai navigateur de l'attaquant peut finir son login", status == 303, status)

print("logout")
sid = login()
cookie = {"Cookie": f"plbls_sid={sid}"}
c.check("GET /auth/logout refusé (405)", req(f"{GW}/auth/logout", headers=cookie)[0] == 405)
c.check("la session est intacte", req(f"{GW}/auth/session", headers=cookie)[0] == 204)
c.done()
