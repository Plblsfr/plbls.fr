import { flushSync } from "svelte";
import { describe, expect, it } from "vitest";
import { fakeApi } from "./fake-api";
import { load } from "./state.svelte";

describe("load", () => {
  it("ne recharge que si le chemin change", async () => {
    const calls = fakeApi({ "GET /v1/apps/chat/roles": { body: [] }, "GET /v1/apps/radar/roles": { body: [] } });
    let app = $state({ key: "chat" });
    const cleanup = $effect.root(() => { load(() => `/v1/apps/${app.key}/roles`); });
    flushSync();
    app = { key: "chat" };
    flushSync();
    expect(calls.map((c) => c.path)).toEqual(["/v1/apps/chat/roles"]);
    app = { key: "radar" };
    flushSync();
    expect(calls.map((c) => c.path)).toEqual(["/v1/apps/chat/roles", "/v1/apps/radar/roles"]);
    cleanup();
  });
});
