// Parcours avec le VRAI auth-gateway et Keycloak (realm de dev de security.plbls.fr) : voir scripts/e2e-gateway.sh.
import { chromium } from "playwright";

const FRONT = process.env.FRONT_URL ?? "http://localhost:3000";
const SHOTS = process.env.SHOTS ?? "/tmp/e2e-shots";
let failed = 0;
const ok = (label, cond) => { if (!cond) failed++; console.log((cond ? "ok   " : "FAIL ") + label); };

const b = await chromium.launch();

async function login(page, user, pass) {
  await page.getByRole("link", { name: "Se connecter" }).click();
  await page.waitForSelector("#username");
  await page.fill("#username", user);
  await page.fill("#password", pass);
  await page.click("#kc-login");
  await page.waitForURL(FRONT + "/**");
}

// Administrateur : rôle realm permission-admin
{
  const page = await (await b.newContext({ viewport: { width: 1280, height: 800 } })).newPage();
  await page.goto(FRONT + "/");
  await page.waitForSelector(".plbls-empty-state");
  ok("sans session : écran de connexion", await page.getByText("Session absente ou expirée").isVisible());
  await login(page, "admin", "admin");
  await page.waitForSelector(".app-row");
  ok("admin connecté : applications visibles via le gateway", (await page.locator(".app-row").count()) >= 2);
  await page.locator(".app-row").filter({ has: page.locator(".app-mono").getByText("chat", { exact: true }) }).click();
  await page.getByRole("button", { name: "+ Nouveau rôle" }).waitFor();
  ok("admin : rôles chargés", (await page.getByRole("button", { name: /·/ }).count()) >= 3);
  await page.screenshot({ path: `${SHOTS}/g1-admin.png`, fullPage: true });
}

// Simple utilisateur : rôle user seulement -> 403 de permission -> écran « accès réservé »
{
  const page = await (await b.newContext()).newPage();
  await page.goto(FRONT + "/");
  await page.waitForSelector(".plbls-empty-state");
  await login(page, "demo", "demo");
  await page.waitForSelector(".plbls-empty-state");
  ok("utilisateur sans permission-admin : accès réservé", await page.getByText("Accès réservé").isVisible());
  await page.screenshot({ path: `${SHOTS}/g2-demo.png` });
}

await b.close();
console.log(failed ? failed + " ÉCHEC(S)" : "TOUT EST OK");
process.exit(failed ? 1 : 0);
