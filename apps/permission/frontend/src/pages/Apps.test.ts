import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { AppInfo, Permission, Role } from "../api";
import { fakeApi } from "../lib/fake-api";
import Apps from "./Apps.svelte";

const APPS: AppInfo[] = [{ key: "chat", name: "Chat", description: "" }, { key: "permission", name: "permission", description: "" }];
const PERMS: Permission[] = [{ key: "message.read", description: "Lire" }, { key: "message.post", description: "" }, { key: "message.delete", description: "Supprimer" }];
const ROLES: Role[] = [{ key: "member", name: "Membre", description: "", permissions: ["message.read"] }];

const routes = () => ({
  "GET /v1/apps": { body: APPS },
  "GET /v1/apps/chat/permissions": { body: PERMS },
  "GET /v1/apps/chat/roles": { body: ROLES },
  "GET /v1/apps/permission/permissions": { body: [] },
  "GET /v1/apps/permission/roles": { body: [] },
});
const ready = () => screen.findByText("chat", { selector: ".app-row .app-mono" });
const row = (key: string) => screen.getByText(key, { selector: ".app-row .app-mono" }).closest("button")!;

describe("Apps", () => {
  it("liste les applications et affiche le détail de celle choisie", async () => {
    fakeApi(routes());
    render(Apps);
    await ready();
    expect(screen.getByText("Choisis une application")).toBeInTheDocument();
    expect(document.querySelectorAll(".app-row")).toHaveLength(2);
    expect(screen.getByText("Nouvelle application").closest("details")).not.toHaveAttribute("open");
    await userEvent.click(row("chat"));
    expect(row("chat")).toHaveAttribute("aria-current", "true");
    expect(row("permission")).toHaveAttribute("aria-current", "false");
    expect(await screen.findByRole("button", { name: "member · 1" })).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByRole("tab", { name: "Rôles" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("button", { name: "Supprimer l'application" })).toBeInTheDocument();
    await userEvent.click(row("permission"));
    expect(await screen.findByText("Aucun rôle pour l'instant.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Supprimer l'application" })).not.toBeInTheDocument();
  });

  it("sans application : formulaire de création ouvert, création puis sélection", async () => {
    let apps: AppInfo[] = [];
    const calls = fakeApi({ ...routes(), "GET /v1/apps": () => ({ body: apps }), "POST /v1/apps": (b) => { apps = [{ ...(b as AppInfo), description: "" }]; return { status: 201, body: b }; }, "GET /v1/apps/radar/permissions": { body: [] }, "GET /v1/apps/radar/roles": { body: [] } });
    render(Apps);
    expect(await screen.findByText("Aucune application. Crée la première ci-dessous.")).toBeInTheDocument();
    expect(screen.getByText("Nouvelle application").closest("details")).toHaveAttribute("open");
    await userEvent.type(screen.getByLabelText("Clé"), "radar");
    await userEvent.type(screen.getByLabelText("Nom"), "Radar");
    await userEvent.click(screen.getByRole("button", { name: "Créer l'application" }));
    await waitFor(() => expect(calls).toContainEqual({ method: "POST", path: "/v1/apps", body: { key: "radar", name: "Radar" } }));
    expect(await screen.findByRole("heading", { name: "Radar radar" })).toBeInTheDocument();
    expect(screen.getByLabelText("Clé")).toHaveValue("");
  });

  it("fiche de rôle : rien n'est enregistré avant « Enregistrer »", async () => {
    let roles = ROLES;
    const calls = fakeApi({ ...routes(), "GET /v1/apps/chat/roles": () => ({ body: roles }), "PUT /v1/apps/chat/roles/member": (b) => { roles = [{ ...roles[0], ...(b as Role) }]; return { body: roles[0] }; } });
    render(Apps);
    await ready();
    await userEvent.click(row("chat"));
    await userEvent.click(await screen.findByRole("button", { name: "member · 1" }));
    const save = screen.getByRole("button", { name: "Enregistrer" });
    expect(save).toBeDisabled();
    expect(document.getElementById("r-key")).toBeDisabled();
    expect(screen.getByText("(1 sur 3)")).toBeInTheDocument();
    const boxes = within(document.querySelector(".app-checks")!).getAllByRole("checkbox");
    expect(boxes.map((b) => (b as HTMLInputElement).checked)).toEqual([true, false, false]);
    await userEvent.click(boxes[2]);
    expect(save).toBeEnabled();
    expect(screen.getByText("Modifications non enregistrées")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Annuler les changements" }));
    expect(save).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Tout cocher" }));
    await userEvent.click(save);
    await waitFor(() => expect(calls.find((c) => c.method === "PUT")?.body).toEqual({ key: "member", name: "Membre", description: "", permissions: ["message.read", "message.post", "message.delete"] }));
    expect(await screen.findByRole("button", { name: "member · 3" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: "Enregistrer" })).toBeDisabled());
  });

  it("changer d'application avec un rôle modifié demande confirmation", async () => {
    fakeApi(routes());
    render(Apps);
    await ready();
    await userEvent.click(row("chat"));
    await userEvent.click(await screen.findByRole("button", { name: "member · 1" }));
    await userEvent.click(screen.getByRole("button", { name: "Tout décocher" }));
    await userEvent.click(row("permission"));
    expect(await screen.findByText("Abandonner les modifications non enregistrées de ce rôle ?")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Annuler" }));
    expect(row("chat")).toHaveAttribute("aria-current", "true");
    await userEvent.click(row("permission"));
    await userEvent.click(await screen.findByRole("button", { name: "Abandonner" }));
    expect(row("permission")).toHaveAttribute("aria-current", "true");
  });

  it("onglet Permissions : ajout, et suppression après confirmation", async () => {
    let perms = PERMS;
    const calls = fakeApi({ ...routes(), "GET /v1/apps/chat/permissions": () => ({ body: perms }),
      "POST /v1/apps/chat/permissions": (b) => { perms = [...perms, b as Permission]; return { status: 201, body: b }; },
      "DELETE /v1/apps/chat/permissions/message.read": () => { perms = perms.slice(1); return {}; } });
    render(Apps);
    await ready();
    await userEvent.click(row("chat"));
    screen.getByRole("tab", { name: "Rôles" }).focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Permissions" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Permissions" })).toHaveFocus();
    expect(await screen.findByText("chat:message.read")).toBeVisible();
    await userEvent.type(screen.getByLabelText("Permission"), "user.ban");
    await userEvent.type(screen.getByLabelText("Description"), "Bannir");
    await userEvent.click(screen.getByRole("button", { name: "Ajouter" }));
    expect(await screen.findByText("chat:user.ban")).toBeInTheDocument();
    expect(calls).toContainEqual({ method: "POST", path: "/v1/apps/chat/permissions", body: { key: "user.ban", description: "Bannir" } });
    await userEvent.click(within(screen.getByText("chat:message.read").closest("tr")!).getByRole("button", { name: "Supprimer" }));
    expect(await screen.findByText("Supprimer la permission chat:message.read ? Elle sera retirée de tous les rôles.")).toBeInTheDocument();
    await userEvent.click(screen.getAllByRole("button", { name: "Supprimer" }).at(-1)!);
    await waitFor(() => expect(screen.queryByText("chat:message.read")).not.toBeInTheDocument());
  });

  it("suppression d'une application après confirmation", async () => {
    let apps = APPS;
    const calls = fakeApi({ ...routes(), "GET /v1/apps": () => ({ body: apps }), "DELETE /v1/apps/chat": () => { apps = apps.slice(1); return {}; } });
    render(Apps);
    await ready();
    await userEvent.click(row("chat"));
    await userEvent.click(await screen.findByRole("button", { name: "Supprimer l'application" }));
    expect(screen.getByText("Supprimer l'application chat, ses permissions, ses rôles et toutes les attributions associées ?")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Supprimer" }));
    await waitFor(() => expect(calls).toContainEqual({ method: "DELETE", path: "/v1/apps/chat", body: undefined }));
    expect(await screen.findByText("Choisis une application")).toBeInTheDocument();
    expect(document.querySelectorAll(".app-row")).toHaveLength(1);
  });
});
