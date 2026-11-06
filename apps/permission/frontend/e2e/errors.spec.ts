import { type Browser, expect, type Route, test } from "@playwright/test";

async function open(browser: Browser, hash: string, handler: (route: Route) => unknown) {
  const page = await (await browser.newContext({ viewport: { width: 1280, height: 800 } })).newPage();
  await page.route("**/api/permission/**", handler);
  await page.goto(`/#${hash}`);
  return page;
}
const empty = (status: number) => (route: Route) => route.fulfill({ status, body: "", headers: {} });

for (const [status, text] of [
  [404, "Route introuvable"],
  [502, "injoignable depuis le gateway"],
  [503, "Authentification indisponible"],
] as const) {
  test(`${status} sans corps : message explicite, ni spinner ni toast vide`, async ({ browser }) => {
    const page = await open(browser, "apps", empty(status));
    await expect(page.locator(".plbls-alert--erreur").first()).toContainText(text);
    await expect(page.locator(".plbls-spinner")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Réessayer" })).toBeVisible();
    const toasts = await page.locator(".plbls-toast").allInnerTexts();
    expect(toasts.every((t) => t.replace("✕", "").trim() !== "")).toBe(true);
  });
}

for (const hash of ["attributions", "audit"]) {
  test(`#${hash} : erreur affichée au lieu d'un spinner infini`, async ({ browser }) => {
    const page = await open(browser, hash, empty(502));
    await expect(page.locator(".plbls-alert--erreur").first()).toBeVisible();
    await expect(page.locator(".plbls-spinner")).toHaveCount(0);
  });
}

test("panne réseau : message explicite", async ({ browser }) => {
  const page = await open(browser, "apps", (route) => route.abort("failed"));
  await expect(page.locator(".plbls-alert--erreur").first()).toContainText("Gateway injoignable");
});

test("403 et 401 remplacent l'écran", async ({ browser }) => {
  const page = await open(browser, "apps", empty(403));
  await expect(page.getByText("Accès réservé")).toBeVisible();
  const page2 = await open(browser, "attributions", empty(403));
  await expect(page2.getByText("Accès réservé")).toBeVisible();
  const page3 = await open(browser, "apps", empty(401));
  await expect(page3.getByText("Session absente ou expirée")).toBeVisible();
});

test("écriture en échec : toast avec un texte", async ({ browser }) => {
  const page = await open(browser, "apps", (route) =>
    route.request().method() === "GET" ? route.continue() : route.fulfill({ status: 502, body: "" }),
  );
  await page.locator(".app-row").first().waitFor();
  await page.getByText("Nouvelle application").click();
  await page.getByLabel("Clé").fill("essai");
  await page.getByLabel("Nom", { exact: true }).fill("Essai");
  await page.getByRole("button", { name: "Créer l'application" }).click();
  const toast = page.locator(".plbls-toast--erreur").first();
  await expect(toast).toBeVisible();
  expect((await toast.innerText()).replace("✕", "").trim().length).toBeGreaterThan(0);
});
