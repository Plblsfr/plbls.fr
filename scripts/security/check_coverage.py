#!/usr/bin/env python3
"""Chaque route du code (src/) et de config/dev.toml doit apparaître dans une requête de la suite Bruno."""
import pathlib
import re
import sys
import tomllib

root = pathlib.Path(__file__).resolve().parents[2] / "api" / "security"
code = "".join(p.read_text() for p in (root / "src").rglob("*.rs"))
bruno = "".join(p.read_text() for p in (root / "bruno").rglob("*.bru"))

routes = set(re.findall(r'\.route\("(/[^"{]*)', code))
routes |= {r["prefix"] for r in tomllib.loads((root / "config/dev.toml").read_text()).get("routes", [])}
routes.discard("/api/")  # joker du proxy, couvert par les préfixes configurés

missing = sorted(r for r in routes if r not in bruno)
for r in missing:
    print(f"non couverte par Bruno : {r}")
print(f"{len(routes) - len(missing)}/{len(routes)} routes couvertes")
sys.exit(1 if missing else 0)
