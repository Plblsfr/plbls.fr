// En prod le front est sur permission.plbls.fr et appelle le gateway (cookie de session .plbls.fr).
// En dev, Vite proxifie /api/permission vers le backend local.
const API = import.meta.env.DEV ? "/api/permission" : "https://security.plbls.fr/api/permission";
// En dev (DEV_VIA_GATEWAY=1) le gateway local de security.plbls.fr écoute sur :8080.
const GATEWAY = import.meta.env.DEV ? "http://localhost:8080" : "https://security.plbls.fr";
export const LOGIN_URL = `${GATEWAY}/auth/login?redirect=${encodeURIComponent(location.href)}`;

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

/** Le gateway répond ses erreurs sans corps, et `statusText` est vide en HTTP/2 : on décrit nous-mêmes le statut. */
function describe(status: number, fromBody: string): string {
  if (fromBody) return fromBody;
  switch (status) {
    case 400: return "Requête refusée par le gateway";
    case 404: return "Route introuvable : /api/permission n'est pas configurée sur le gateway";
    case 502: return "Le service permission est injoignable depuis le gateway";
    case 503: return "Authentification indisponible, réessaie dans un instant";
    case 504: return "Le service permission ne répond pas (délai dépassé)";
  }
  return `Erreur HTTP ${status}`;
}

export async function api<T = void>(method: string, path: string, body?: unknown): Promise<T> {
  let res: Response;
  try {
    res = await fetch(API + path, {
      method,
      credentials: "include",
      headers: body === undefined ? {} : { "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new ApiError(0, "Gateway injoignable (réseau, CORS ou certificat)");
  }
  if (!res.ok) {
    let msg = "";
    try {
      const j = JSON.parse(await res.text());
      msg = typeof j.error === "string" ? j.error : typeof j.message === "string" ? j.message : "";
    } catch { /* corps vide ou non JSON (HTML d'un proxy) */ }
    throw new ApiError(res.status, describe(res.status, msg));
  }
  return res.status === 204 ? (undefined as T) : res.json();
}

export interface AppInfo { key: string; name: string; description: string }
export interface Permission { key: string; description: string }
export interface Role { key: string; name: string; description: string; permissions: string[] }
export interface Assignment { user_id: string; app: string; role: string; granted_by: string; created_at: string }
export interface AuditEntry { at: string; actor: string; action: string; target: string; detail: unknown }
export interface Permissions { user_id: string; permissions: string[] }
export interface FoundUsers { users: { id: string; email: string }[]; first: number; max: number; next: number | null }
