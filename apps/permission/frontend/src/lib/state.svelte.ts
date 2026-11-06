import { untrack } from "svelte";
import { api, ApiError } from "../api";
import { toast } from "../vendor/plbls-ui/plbls-ui.esm.min.js";

export type Gate = "login" | "forbidden";
export const gate = $state<{ value: Gate | null }>({ value: null });

export function handle(e: unknown): string | null {
  if (e instanceof ApiError && e.status === 401) { gate.value = "login"; return null; }
  if (e instanceof ApiError && e.status === 403) { gate.value = "forbidden"; return null; }
  return e instanceof Error && e.message ? e.message : "Erreur inattendue";
}

export function fail(e: unknown) {
  const msg = handle(e);
  if (msg) toast.error(msg);
}

export function run(p: Promise<unknown>, done?: string): Promise<boolean> {
  return p.then(() => { if (done) toast.success(done); return true; }, (e) => { fail(e); return false; });
}

export interface Loaded<T> { readonly data: T | null; readonly error: string | null; reload: () => void }

export function load<T>(path: () => string | null): Loaded<T> {
  let data = $state.raw<T | null>(null);
  let error = $state<string | null>(null);
  let current: string | null = null;
  const reload = () => {
    const p = current;
    if (!p) { data = null; return; }
    error = null;
    api<T>("GET", p).then(
      (d) => { if (current === p) data = d; },
      (e) => { if (current === p) error = handle(e); },
    );
  };
  $effect(() => {
    const p = path();
    if (p === current) return;
    current = p;
    data = null;
    untrack(reload);
  });
  return { get data() { return data; }, get error() { return error; }, reload };
}
