// Identité : le cookie de session de security.plbls.fr (Domain=.plbls.fr) arrive aussi sur pli.plbls.fr.
// Pli le transmet au gateway (GET /auth/me) qui répond avec l'identité, ou 401.
// Renvoie { user } ou { reason } (la raison aide à diagnostiquer un 401, elle ne contient aucun secret).
export async function fetchIdentity(cookie, { gatewayUrl, fetchFn = fetch }) {
  if (!cookie) return { reason: 'aucun cookie de session reçu par pli' };
  try {
    const res = await fetchFn(`${gatewayUrl.replace(/\/$/, '')}/auth/me`, {
      headers: { cookie, 'user-agent': 'pli.plbls.fr' }, signal: AbortSignal.timeout(5000) });
    if (!res.ok) {
      const body = (await res.text?.().catch(() => '') || '').replace(/\s+/g, ' ').slice(0, 100);
      return { reason: `le gateway a répondu HTTP ${res.status} à /auth/me (serveur : ${res.headers?.get?.('server') || '?'}, corps : ${body || 'vide'})` };
    }
    const me = await res.json();
    return me.id ? { user: me } : { reason: 'réponse /auth/me sans id' };
  } catch (e) { return { reason: `gateway injoignable depuis pli (${e.cause?.code || e.message})` }; }
}

// Anti-CSRF : une requête qui modifie doit venir de l'origine de pli lui-même.
export function sameOrigin(headers) {
  const origin = headers.origin;
  if (!origin) return false;
  try { return new URL(origin).host === (headers['x-forwarded-host'] || headers.host); } catch { return false; }
}

// Demande à permission.plbls.fr si le compte a la permission (token de service, lecture seule).
// Cache court par compte ; en cas d'erreur réseau ou de réponse inattendue : refus.
// Renvoie { allowed, reason } (reason seulement quand la réponse n'est pas un vrai « oui/non »).
export function permissionChecker({ url, token, permission = 'pli:deposit', ttlMs = 30_000, fetchFn = fetch }) {
  const cache = new Map();
  return async (userId, now = Date.now()) => {
    const hit = cache.get(userId);
    if (hit && hit.until > now) return { allowed: hit.allowed };
    try {
      const res = await fetchFn(`${url.replace(/\/$/, '')}/v1/check`, {
        method: 'POST',
        headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
        body: JSON.stringify({ user_id: userId, permission }),
        signal: AbortSignal.timeout(5000),
      });
      if (!res.ok) return { allowed: false, reason: `permission a répondu HTTP ${res.status}${res.status === 401 ? ' (token de service refusé : vérifier PERMISSION_SERVICE_TOKEN et PERMISSION__SERVICES__TOKENS)' : ''}` };
      const allowed = (await res.json()).allowed === true;
      cache.set(userId, { allowed, until: now + ttlMs });
      return { allowed };
    } catch (e) { return { allowed: false, reason: `permission injoignable depuis pli (${e.cause?.code || e.message})` }; }
  };
}
