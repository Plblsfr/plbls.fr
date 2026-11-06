"""Outils communs aux scripts de dev : signature d'identité comme auth-gateway, appels JSON."""
import base64, hashlib, hmac, json, os, time, urllib.error, urllib.request

BASE = os.environ.get("BASE", "http://127.0.0.1:8090")
SECRET = os.environ.get("PERMISSION__IDENTITY__SECRET", "dev-identity-secret")

def sign(uid, roles):
    ts = int(time.time())
    mac = hmac.new(SECRET.encode(), f"{uid}\n{roles}\nplbls-gateway\n{ts}".encode(), hashlib.sha256).digest()
    return {"X-User-Id": uid, "X-User-Roles": roles, "X-Client-Id": "plbls-gateway", "X-Identity-Ts": str(ts),
            "X-Identity-Sig": base64.urlsafe_b64encode(mac).rstrip(b"=").decode()}

def call(method, path, headers, body=None):
    req = urllib.request.Request(BASE + path, method=method, headers={"Content-Type": "application/json", **headers},
                                 data=json.dumps(body).encode() if body is not None else None)
    try:
        with urllib.request.urlopen(req) as r:
            raw = r.read(); return r.status, json.loads(raw) if raw else None
    except urllib.error.HTTPError as e:
        raw = e.read(); return e.code, json.loads(raw) if raw else None
