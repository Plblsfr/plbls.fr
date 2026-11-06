import { expect, type Page, test } from "@playwright/test";

const appRow = (p: Page, key: string) =>
  p.locator(".app-row").filter({ has: p.locator(".app-mono").getByText(key, { exact: true }) });

test("parcours d'administration sans erreur console", async ({ page }) => {
  const errors: string[] = [];
  page.on("console", (m) => {
    if (m.type() === "error" && !m.text().includes("status of 400")) errors.push(`console: ${m.text()}`);
  });
  page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
  await page.setViewportSize({ width: 1280, height: 900 });

  await page.goto("/");
  await expect(page.locator(".app-row"), "applications listées (chat, radar et la réservée permission)").toHaveCount(3);

  await appRow(page, "chat").click();
  await page.getByRole("button", { name: /^member/ }).click();
  const checks = page.locator(".app-checks li");
  await expect(checks.nth(2)).toBeVisible();
  expect(await checks.count(), "fiche du rôle : une case par permission").toBeGreaterThanOrEqual(3);
  const save = page.getByRole("button", { name: "Enregistrer", exact: true });
  await expect(save, "Enregistrer désactivé tant que rien n'a changé").toBeDisabled();
  const cb = checks.nth(2).locator("input");
  const before = await cb.isChecked();
  await checks.nth(2).locator("label.plbls-checkbox").click();
  await expect(save, "cocher ne sauvegarde pas encore").toBeEnabled();
  await save.click();
  await expect(page.locator(".plbls-toast").first(), "enregistrer déclenche un toast de succès").toBeVisible();
  await expect(save).toBeDisabled();
  expect(await cb.isChecked(), "la case a changé et reste après rechargement des données").toBe(!before);
  await checks.nth(2).locator("label.plbls-checkbox").click();
  await save.click();
  await expect(save).toBeDisabled();

  await page.getByRole("button", { name: "Supprimer l'application" }).click();
  await expect(page.locator(".plbls-modal"), "modale de confirmation visible").toBeVisible();
  await page.getByRole("button", { name: "Annuler", exact: true }).click();
  await expect(page.locator(".plbls-modal"), "annuler ferme la modale").toHaveCount(0);
  await expect(page.locator(".app-cols > .plbls-panel .app-row"), "l'application existe toujours").toHaveCount(3);

  await page.getByRole("tab", { name: "Permissions" }).click();
  await page.getByLabel("Permission", { exact: true }).fill("Bad Key");
  await page.getByLabel("Permission", { exact: true }).press("Enter");
  await expect(page.locator(".plbls-toast--erreur"), "clé invalide : toast d'erreur").toBeVisible();

  await page.getByRole("link", { name: "Attributions" }).click();
  await expect(page.locator("tbody tr .plbls-status--accent").first()).toBeVisible();
  expect(await page.locator("tbody tr").count(), "attributions affichées").toBeGreaterThanOrEqual(3);

  await page.getByRole("link", { name: "Journal" }).click();
  await expect(page.locator("tbody tr").first(), "journal affiché").toBeVisible();

  expect(errors, "aucune erreur console inattendue").toEqual([]);
});

test("pas de défilement horizontal en mobile", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 800 });
  await page.goto("/#apps");
  await appRow(page, "chat").click();
  await page.getByRole("button", { name: "+ Nouveau rôle" }).waitFor();
  expect(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1)).toBe(false);
});
