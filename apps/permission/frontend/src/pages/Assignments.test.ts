import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Assignment } from "../api";
import { fakeApi } from "../lib/fake-api";
import Assignments from "./Assignments.svelte";

const ROWS: Assignment[] = [{ user_id: "u-1", app: "chat", role: "member", granted_by: "dev-admin", created_at: "2026-01-02T10:00:00Z" }];
const routes = (perms: string[] = []) => ({
  "GET /v1/assignments": { body: ROWS },
  "GET /v1/apps": { body: [{ key: "chat", name: "Chat", description: "" }] },
  "GET /v1/apps/chat/roles": { body: [{ key: "member", name: "Membre", description: "", permissions: [] }, { key: "admin", name: "Admin", description: "", permissions: [] }] },
  "GET /v1/me/permissions": { body: { user_id: "dev-admin", permissions: perms } },
});

describe("Assignments", () => {
  it("liste les attributions et filtre par compte", async () => {
    const calls = fakeApi({ ...routes(), "GET /v1/assignments?user_id=u-2": { body: [] } });
    render(Assignments);
    const row = (await screen.findByText("u-1")).closest("tr")!;
    expect(within(row).getByText("member")).toHaveClass("plbls-status--accent");
    expect(within(row).getByText(new Date(ROWS[0].created_at).toLocaleString("fr-FR"))).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Filtrer par compte"), "u-2");
    expect(await screen.findByText("Ce compte n'a aucun rôle.")).toBeInTheDocument();
    expect(calls.map((c) => c.path)).toContain("/v1/assignments?user_id=u-2");
  });

  it("sans users.read : saisie du sub, choix de l'application puis du rôle, attribution", async () => {
    const calls = fakeApi({ ...routes(), "POST /v1/assignments": { status: 201, body: {} } });
    render(Assignments);
    await screen.findByText("u-1");
    expect(screen.getByLabelText("Compte (sub Keycloak)")).toBeInTheDocument();
    const attribuer = screen.getByRole("button", { name: "Attribuer" });
    expect(attribuer).toBeDisabled();
    expect(screen.getByLabelText("Rôle")).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Compte (sub Keycloak)"), "u-9");
    await userEvent.selectOptions(screen.getByLabelText("Application"), await screen.findByRole("option", { name: "chat" }));
    await userEvent.selectOptions(screen.getByLabelText("Rôle"), await screen.findByRole("option", { name: "admin" }));
    await userEvent.click(attribuer);
    await waitFor(() => expect(calls).toContainEqual({ method: "POST", path: "/v1/assignments", body: { user_id: "u-9", app: "chat", role: "admin" } }));
    await waitFor(() => expect(calls.filter((c) => c.path === "/v1/assignments")).toHaveLength(3));
  });

  it("changer d'application remet le rôle à « Choisir… »", async () => {
    fakeApi(routes());
    render(Assignments);
    await userEvent.selectOptions(screen.getByLabelText("Application"), await screen.findByRole("option", { name: "chat" }));
    await userEvent.selectOptions(screen.getByLabelText("Rôle"), await screen.findByRole("option", { name: "member" }));
    await userEvent.selectOptions(screen.getByLabelText("Application"), "");
    expect(screen.getByLabelText("Rôle")).toHaveValue("");
    expect(screen.getByLabelText("Rôle")).toBeDisabled();
  });

  it("avec users.read : recherche par e-mail, pages suivantes et choix du compte", async () => {
    const calls = fakeApi({ ...routes(["permission:users.read"]),
      "GET /v1/users?email=ali&first=0": { body: { users: [{ id: "s-1", email: "alice@x.fr" }], first: 0, max: 1, next: 1 } },
      "GET /v1/users?email=ali&first=1": { body: { users: [{ id: "s-2", email: "alix@x.fr" }], first: 1, max: 1, next: null } } });
    render(Assignments);
    const search = await screen.findByLabelText("Compte (e-mail)");
    await userEvent.type(search, "ali");
    expect(await screen.findByText("alice@x.fr", {}, { timeout: 2000 })).toBeInTheDocument();
    expect(calls.filter((c) => c.path.startsWith("/v1/users"))).toHaveLength(1);
    await userEvent.click(screen.getByRole("button", { name: "Plus de résultats" }));
    expect(await screen.findByText("alix@x.fr")).toBeInTheDocument();
    expect(screen.getByText("alice@x.fr")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Plus de résultats" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByText("alix@x.fr").closest("button")!);
    expect(screen.getByText("alix@x.fr")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Attribuer" })).toBeEnabled();
    await userEvent.click(screen.getByRole("button", { name: "Changer" }));
    expect(screen.getByLabelText("Compte (e-mail)")).toHaveValue("");
    expect(screen.getByRole("button", { name: "Attribuer" })).toBeDisabled();
  });

  it("recherche sans résultat : « Aucun compte. »", async () => {
    fakeApi({ ...routes(["permission:users.read"]), "GET /v1/users?email=zz&first=0": { body: { users: [], first: 0, max: 20, next: null } } });
    render(Assignments);
    await userEvent.type(await screen.findByLabelText("Compte (e-mail)"), "zz");
    expect(await screen.findByText("Aucun compte.", {}, { timeout: 2000 })).toBeInTheDocument();
  });

  it("retrait après confirmation", async () => {
    let rows = ROWS;
    const calls = fakeApi({ ...routes(), "GET /v1/assignments": () => ({ body: rows }), "DELETE /v1/assignments?user_id=u-1&app=chat&role=member": () => { rows = []; return {}; } });
    render(Assignments);
    await userEvent.click(within((await screen.findByText("u-1")).closest("tr")!).getByRole("button", { name: "Retirer" }));
    expect(screen.getByText("Retirer chat/member au compte u-1 ?")).toBeInTheDocument();
    await userEvent.click(screen.getAllByRole("button", { name: "Retirer" }).at(-1)!);
    expect(await screen.findByText("Personne n'a encore de rôle.")).toBeInTheDocument();
    expect(calls.some((c) => c.method === "DELETE")).toBe(true);
  });
});
