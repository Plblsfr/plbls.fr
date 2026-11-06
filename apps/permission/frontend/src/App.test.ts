import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import App from "./App.svelte";
import { fakeApi } from "./lib/fake-api";
import { gate } from "./lib/state.svelte";

beforeEach(() => { gate.value = null; location.hash = ""; });

describe("App", () => {
  it("navigue par le hash et marque la page courante", async () => {
    fakeApi({ "GET /v1/apps": { body: [] }, "GET /v1/audit?limit=200": { body: [] } });
    render(App);
    expect(screen.getByRole("link", { name: "Applications" })).toHaveAttribute("aria-current", "page");
    expect(await screen.findByText("Aucune application. Crée la première ci-dessous.")).toBeInTheDocument();
    location.hash = "#audit";
    window.dispatchEvent(new HashChangeEvent("hashchange"));
    expect(await screen.findByText("Rien à signaler")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Journal" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("link", { name: "Applications" })).not.toHaveAttribute("aria-current");
  });

  it("referme le menu mobile au choix d'un lien, au clic à côté et sur Échap", async () => {
    fakeApi({ "GET /v1/apps": { body: [] } });
    const { container } = render(App);
    const side = container.querySelector(".plbls-sidebar")!;
    const open = () => { side.classList.add("is-open"); side.querySelector(".plbls-sidebar__bascule")!.textContent = "Fermer"; };
    open();
    await userEvent.click(screen.getByRole("main"));
    expect(side).not.toHaveClass("is-open");
    expect(screen.getByRole("button", { name: "Menu" })).toHaveAttribute("aria-expanded", "false");
    open();
    await userEvent.keyboard("{Escape}");
    expect(side).not.toHaveClass("is-open");
  });

  it("401 : écran de connexion", async () => {
    fakeApi({ "GET /v1/apps": { status: 401 } });
    render(App);
    expect(await screen.findByText("Session absente ou expirée")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Se connecter" }).getAttribute("href")).toMatch(/^http:\/\/localhost:8080\/auth\/login\?redirect=/);
  });

  it("403 : accès réservé", async () => {
    fakeApi({ "GET /v1/apps": { status: 403 } });
    render(App);
    expect(await screen.findByText("Accès réservé")).toBeInTheDocument();
    expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
  });
});
