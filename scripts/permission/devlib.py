"""Outils des scripts de dev de permission : signature d'identité comme auth-gateway, appels JSON."""
import base64
import hashlib
import hmac
import os
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts" / "python"))
from devcommun import call_json, req

__all__ = ["BASE", "SECRET", "call", "req", "sign"]

BASE = os.environ.get("BASE", "http://127.0.0.1:8090")
SECRET = os.environ.get("PERMISSION__IDENTITY__SECRET", "dev-identity-secret")

def sign(uid, roles):
    ts = int(time.time())
    mac = hmac.new(SECRET.encode(), f"{uid}\n{roles}\nplbls-gateway\n{ts}".encode(), hashlib.sha256).digest()
    return {"X-User-Id": uid, "X-User-Roles": roles, "X-Client-Id": "plbls-gateway", "X-Identity-Ts": str(ts),
            "X-Identity-Sig": base64.urlsafe_b64encode(mac).rstrip(b"=").decode()}

def call(method, path, headers, body=None):
    return call_json(BASE + path, method, headers, body)
