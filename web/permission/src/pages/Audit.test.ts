import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeApi } from "../lib/fake-api";
import Audit from "./Audit.svelte";

describe("Audit", () => {
  it("affiche le journal avec la tonalité de chaque action", async () => {
    fakeApi({ "GET /v1/audit?limit=200": { body: [
      { at: "2026-01-02T10:00:00Z", actor: "dev-admin", action: "assignment.grant", target: "u1", detail: { role: "member" } },
      { at: "2026-01-02T11:00:00Z", actor: "dev-admin", action: "role.delete", target: "chat/admin", detail: null },
      { at: "2026-01-02T12:00:00Z", actor: "dev-admin", action: "app.other", target: "chat", detail: 1 },
    ] } });
    render(Audit);
    const rows = await screen.findAllByRole("row");
    expect(rows).toHaveLength(4);
    expect(within(rows[1]).getByText("assignment.grant")).toHaveClass("plbls-status--positif");
    expect(within(rows[1]).getByText('{"role":"member"}')).toBeInTheDocument();
    expect(within(rows[2]).getByText("role.delete")).toHaveClass("plbls-status--negatif");
    expect(within(rows[3]).getByText("app.other")).toHaveClass("plbls-status", "plbls-status--");
    expect(within(rows[1]).getByText(new Date("2026-01-02T10:00:00Z").toLocaleString("fr-FR"))).toBeInTheDocument();
  });

  it("journal vide : état vide", async () => {
    fakeApi({ "GET /v1/audit?limit=200": { body: [] } });
    render(Audit);
    expect(await screen.findByText("Rien à signaler")).toBeInTheDocument();
  });

  it("chargement puis erreur : message et Réessayer", async () => {
    let fail = true;
    fakeApi({ "GET /v1/audit?limit=200": () => (fail ? { status: 502 } : { body: [] }) });
    render(Audit);
    expect(screen.getByRole("status", { name: "Chargement" })).toBeInTheDocument();
    expect(await screen.findByRole("alert")).toHaveTextContent("Le service permission est injoignable depuis le gateway");
    fail = false;
    await userEvent.click(screen.getByRole("button", { name: "Réessayer" }));
    expect(await screen.findByText("Rien à signaler")).toBeInTheDocument();
  });
});
