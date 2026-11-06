// Limiteur de débit en mémoire (fenêtre fixe) : freine l'énumération d'identifiants sur la lecture publique.
export function rateLimiter({ max, windowMs }) {
  const hits = new Map();
  return (key, now = Date.now()) => {
    const h = hits.get(key);
    if (!h || h.until <= now) {
      if (hits.size > 10_000) for (const [k, v] of hits) if (v.until <= now) hits.delete(k);
      hits.set(key, { n: 1, until: now + windowMs });
      return true;
    }
    return ++h.n <= max;
  };
}

// Derrière le reverse proxy : X-Forwarded-For (dernier saut ajouté par le proxy), sinon l'adresse du socket.
export function clientIp(req) {
  const xff = req.headers['x-forwarded-for'];
  if (xff) return String(xff).split(',').pop().trim();
  return req.socket.remoteAddress || '?';
}
