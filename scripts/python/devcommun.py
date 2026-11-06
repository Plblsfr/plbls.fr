"""Outils communs aux scripts Python de dev et de test des projets (stdlib seulement) : HTTP sans redirection, vérifications."""

import json
import urllib.error
import urllib.request


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


def call_json(url, method="GET", headers=None, body=None):
    """Requête JSON : retourne (status, corps décodé ou None)."""
    data = json.dumps(body).encode() if body is not None else None
    status, _, raw = req(url, method, {"Content-Type": "application/json", **(headers or {})}, data)
    return status, json.loads(raw) if raw else None


class Checks:
    def __init__(self):
        self.failed = 0

    def check(self, name, ok, detail=""):
        print(("  ✓ " if ok else "  ✗ ") + name + ("" if ok else f"  -> {detail}"))
        self.failed += not ok

    def done(self):
        print("OK" if not self.failed else f"{self.failed} échec(s)")
        raise SystemExit(1 if self.failed else 0)
