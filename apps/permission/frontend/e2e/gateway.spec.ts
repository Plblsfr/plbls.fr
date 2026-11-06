import { expect, type Page, test } from "@playwright/test";
import { front } from "./ports";

async function login(page: Page, user: string, pass: string) {
  await page.getByRole("link", { name: "Se connecter" }).click();
  await page.fill("#username", user);
  await page.fill("#password", pass);
  await page.click("#kc-login");
  await page.waitForURL(`http://localhost:${front}/**`);
}

test("administrateur : connexion par le gateway, applications et rôles visibles", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Session absente ou expirée")).toBeVisible();
  await login(page, "admin", "admin");
  await expect(page.locator(".app-row").nth(1)).toBeVisible();
  await page
    .locator(".app-row")
    .filter({ has: page.locator(".app-mono").getByText("chat", { exact: true }) })
    .click();
  await page.getByRole("button", { name: "+ Nouveau rôle" }).waitFor();
  expect(await page.getByRole("button", { name: /·/ }).count()).toBeGreaterThanOrEqual(3);
});

test("utilisateur sans permission-admin : accès réservé", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator(".plbls-empty-state")).toBeVisible();
  await login(page, "demo", "demo");
  await expect(page.getByText("Accès réservé")).toBeVisible();
});
