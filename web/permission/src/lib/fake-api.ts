import { vi } from "vitest";

type Reply = { status?: number; body?: unknown } | ((body: unknown) => { status?: number; body?: unknown });

export interface Call { method: string; path: string; body: unknown }

export function fakeApi(routes: Record<string, Reply>) {
  const calls: Call[] = [];
  const fetch = vi.fn(async (url: string, init: RequestInit = {}) => {
    const method = init.method ?? "GET";
    const path = url.replace(/^\/api\/permission/, "");
    const body = typeof init.body === "string" ? JSON.parse(init.body) : undefined;
    calls.push({ method, path, body });
    const route = routes[`${method} ${path}`] ?? routes[`${method} ${path.split("?")[0]}`];
    if (!route) return new Response(JSON.stringify({ error: `route absente : ${method} ${path}` }), { status: 500 });
    const r = typeof route === "function" ? route(body) : route;
    return new Response(r.body === undefined ? null : JSON.stringify(r.body), { status: r.status ?? (r.body === undefined ? 204 : 200) });
  });
  vi.stubGlobal("fetch", fetch);
  return calls;
}
