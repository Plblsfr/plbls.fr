import { describe, expect, it, vi } from "vitest";
import { api, ApiError, LOGIN_URL } from "./api";

const reply = (status: number, body: string | null) => vi.stubGlobal("fetch", vi.fn(async () => new Response(body, { status })));

describe("api", () => {
  it("appelle /api/permission avec le cookie et le corps JSON", async () => {
    const fetch = vi.fn(async () => new Response(JSON.stringify({ ok: 1 }), { status: 200 }));
    vi.stubGlobal("fetch", fetch);
    await expect(api("POST", "/v1/apps", { key: "chat" })).resolves.toEqual({ ok: 1 });
    expect(fetch).toHaveBeenCalledWith("/api/permission/v1/apps", {
      method: "POST", credentials: "include", headers: { "Content-Type": "application/json" }, body: '{"key":"chat"}',
    });
  });

  it("n'envoie ni corps ni Content-Type sans body", async () => {
    const fetch = vi.fn(async () => new Response("[]", { status: 200 }));
    vi.stubGlobal("fetch", fetch);
    await api("GET", "/v1/apps");
    expect(fetch).toHaveBeenCalledWith("/api/permission/v1/apps", { method: "GET", credentials: "include", headers: {}, body: undefined });
  });

  it("204 : renvoie undefined", async () => {
    reply(204, null);
    await expect(api("DELETE", "/v1/apps/chat")).resolves.toBeUndefined();
  });

  it("erreur JSON : reprend error puis message", async () => {
    reply(400, JSON.stringify({ error: "clé invalide" }));
    await expect(api("POST", "/v1/apps")).rejects.toMatchObject({ status: 400, message: "clé invalide" });
    reply(409, JSON.stringify({ message: "existe déjà" }));
    await expect(api("POST", "/v1/apps")).rejects.toMatchObject({ status: 409, message: "existe déjà" });
  });

  it.each([
    [400, "Requête refusée par le gateway"],
    [404, "Route introuvable : /api/permission n'est pas configurée sur le gateway"],
    [502, "Le service permission est injoignable depuis le gateway"],
    [503, "Authentification indisponible, réessaie dans un instant"],
    [504, "Le service permission ne répond pas (délai dépassé)"],
    [500, "Erreur HTTP 500"],
  ])("statut %i sans corps : « %s »", async (status, message) => {
    reply(status, status === 500 ? "<html>proxy</html>" : "");
    const e = await api("GET", "/v1/apps").catch((x) => x);
    expect(e).toBeInstanceOf(ApiError);
    expect(e).toMatchObject({ status, message });
  });

  it("réseau en panne : statut 0", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => { throw new TypeError("Failed to fetch"); }));
    await expect(api("GET", "/v1/apps")).rejects.toMatchObject({ status: 0, message: "Gateway injoignable (réseau, CORS ou certificat)" });
  });

  it("LOGIN_URL renvoie vers le gateway avec la page courante", () => {
    expect(LOGIN_URL).toBe(`http://localhost:8080/auth/login?redirect=${encodeURIComponent(location.href)}`);
  });
});
