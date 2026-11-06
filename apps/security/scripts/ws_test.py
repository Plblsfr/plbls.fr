#!/usr/bin/env python3
"""Proxy WebSocket vers traefik/whoami (/echo) : auth avant upgrade, Origin, aller-retour."""
import base64, os, socket

from devlib import Checks, login, password_token

c = Checks()


def ws_connect(path, headers):
    s = socket.create_connection(("127.0.0.1", 8080), timeout=5)
    key = base64.b64encode(os.urandom(16)).decode()
    lines = [f"GET {path} HTTP/1.1", "Host: localhost:8080", "Upgrade: websocket", "Connection: Upgrade",
             f"Sec-WebSocket-Key: {key}", "Sec-WebSocket-Version: 13", *[f"{k}: {v}" for k, v in headers.items()]]
    s.sendall(("\r\n".join(lines) + "\r\n\r\n").encode())
    buf = b""
    while b"\r\n\r\n" not in buf:
        buf += s.recv(4096)
    head, rest = buf.split(b"\r\n\r\n", 1)
    return s, int(head.split()[1]), rest


def send_text(s, text):
    data, mask = text.encode(), os.urandom(4)
    s.sendall(bytes([0x81, 0x80 | len(data)]) + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))


def recv_text(s, rest=b""):
    buf = rest
    while len(buf) < 2:
        buf += s.recv(4096)
    n = buf[1] & 0x7F
    while len(buf) < 2 + n:
        buf += s.recv(4096)
    return buf[2:2 + n].decode()


sid = login()
cookie = {"Cookie": f"plbls_sid={sid}"}
ORIGIN = {"Origin": "http://localhost:3000"}

s, st, _ = ws_connect("/api/private/echo", {})
c.check("sans authentification -> 401", st == 401, st)
s.close()

s, st, _ = ws_connect("/api/whoami/echo", {})
c.check("route publique sans authentification -> 101", st == 101, st)
s.close()

s, st, _ = ws_connect("/api/private/echo", {**cookie, "Origin": "https://evil.example"})
c.check("cookie + Origin étranger -> 403", st == 403, st)
s.close()

s, st, _ = ws_connect("/api/private/echo", cookie)
c.check("cookie sans Origin -> 403", st == 403, st)
s.close()

s, st, rest = ws_connect("/api/private/echo", {**cookie, **ORIGIN})
c.check("cookie + Origin autorisé -> 101", st == 101, st)
if st == 101:
    send_text(s, "bonjour")
    c.check("écho relayé", recv_text(s, rest) == "bonjour")
    # durée de vie maximale (4 s en dev) : le gateway ferme avec 1001 pour forcer la reconnexion
    s.settimeout(10)
    frame = s.recv(64)
    c.check("fermeture 1001 après la durée de vie maximale", frame[:1] == b"\x88" and frame[2:4] == b"\x03\xe9", frame)
s.close()

s, st, rest = ws_connect("/api/private/echo", {"Authorization": "Bearer " + password_token()})
c.check("Bearer sans Origin -> 101", st == 101, st)
s.close()
c.done()
