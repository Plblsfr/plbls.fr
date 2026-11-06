"""Utilitaires des scripts de test de security (stdlib seulement) : login BFF complet, Redis via docker, tokens."""
import hashlib
import json
import re
import subprocess
import sys
import urllib.parse
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts" / "python"))
from devcommun import Checks, req

__all__ = ["GW", "KC", "REALM", "Checks", "login", "password_token", "redis", "req", "session_key"]

GW = "http://localhost:8080"
KC = "http://localhost:8180"
REALM = f"{KC}/realms/plbls/protocol/openid-connect"


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
    return subprocess.run(["docker", "exec", "security-dev-redis-1", "redis-cli", *args], capture_output=True, text=True, check=True).stdout.strip()


def session_key(sid):
    return "sess:" + hashlib.sha256(sid.encode()).hexdigest()
