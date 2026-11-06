"""Utilitaires communs aux scripts de test (stdlib seulement) : login BFF complet, Redis via docker, tokens."""
import hashlib, http.client, json, re, subprocess, urllib.parse, urllib.request

GW = "http://localhost:8080"
KC = "http://localhost:8180"
REALM = f"{KC}/realms/plbls/protocol/openid-connect"


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *a, **k):
        return None


_opener = urllib.request.build_opener(_NoRedirect)


def req(url, method="GET", headers=None, data=None):
    """Retourne (status, headers, body) sans suivre les redirections."""
    r = urllib.request.Request(url, method=method, headers=headers or {}, data=data)
    try:
        resp = _opener.open(r, timeout=30)
    except urllib.error.HTTPError as e:
        resp = e
    return resp.status if hasattr(resp, "status") else resp.code, resp.headers, resp.read()


def login(user="demo", password="demo"):
    """Parcourt /auth/login -> Keycloak -> /auth/callback, retourne le sid."""
    _, h, _ = req(f"{GW}/auth/login?redirect=http://localhost:3000/")
    login_cookie = next(c for c in h.get_all("Set-Cookie") if c.startswith("plbls_sid_login=")).split(";")[0]
    _, h2, body = req(h["Location"])
    kc_cookie = "; ".join(c.split(";")[0] for c in h2.get_all("Set-Cookie"))
    action = re.search(r'action="([^"]+)"', body.decode()).group(1).replace("&amp;", "&")
    form = urllib.parse.urlencode({"username": user, "password": password, "credentialId": ""}).encode()
    _, h3, _ = req(action, "POST", {"Cookie": kc_cookie, "Content-Type": "application/x-www-form-urlencoded"}, form)
    _, h4, _ = req(h3["Location"], headers={"Cookie": login_cookie})
    sid = next(c for c in h4.get_all("Set-Cookie") if c.startswith("plbls_sid=")).split(";")[0].split("=", 1)[1]
    return sid


def password_token(client="plbls-mobile", user="demo", password="demo"):
    """Access token par password grant (clients de test uniquement)."""
    form = urllib.parse.urlencode({"grant_type": "password", "client_id": client, "username": user, "password": password}).encode()
    s, _, body = req(f"{REALM}/token", "POST", {"Content-Type": "application/x-www-form-urlencoded"}, form)
    assert s == 200, body
    return json.loads(body)["access_token"]


def redis(*args):
    return subprocess.run(["docker", "exec", "deploy-redis-1", "redis-cli", *args], capture_output=True, text=True, check=True).stdout.strip()


def session_key(sid):
    return "sess:" + hashlib.sha256(sid.encode()).hexdigest()


class Checks:
    def __init__(self):
        self.failed = 0

    def check(self, name, ok, detail=""):
        print(("  ✓ " if ok else "  ✗ ") + name + ("" if ok else f"  -> {detail}"))
        self.failed += not ok

    def done(self):
        print("OK" if not self.failed else f"{self.failed} échec(s)")
        raise SystemExit(1 if self.failed else 0)
