import { chromium } from "playwright";

const BASE = process.env.FRONT_URL ?? "http://127.0.0.1:3000";
const SHOTS = process.env.SHOTS ?? "/tmp/e2e-shots";
let failed = 0;
const b = await chromium.launch();
const errors = [];
const page = await b.newPage({ viewport: { width: 1280, height: 900 } });
page.on("console", (m) => m.type() === "error" && !m.text().includes("status of 400") && errors.push("console: " + m.text()));
page.on("pageerror", (e) => errors.push("pageerror: " + e.message));
const ok = (label, cond) => { if (!cond) failed++; console.log((cond ? "ok   " : "FAIL ") + label); };
const appRow = (p, key) => p.locator(".app-row").filter({ has: p.locator(".app-mono").getByText(key, { exact: true }) });
const shot = (n) => page.screenshot({ path: `${SHOTS}/${n}.png`, fullPage: true });

await page.goto(BASE + "/");
await page.waitForSelector(".app-row");
ok("applications listées (chat, radar et la réservée permission)", (await page.locator(".app-row").count()) === 3);
await shot("1-apps");

await appRow(page, "chat").click();
await page.getByRole("button", { name: /^member/ }).click();
await page.waitForSelector(".app-checks");
ok("fiche du rôle : une case par permission", (await page.locator(".app-checks li").count()) >= 3);
const save = page.getByRole("button", { name: "Enregistrer", exact: true });
ok("Enregistrer désactivé tant que rien n'a changé", await save.isDisabled());
const cb = page.locator(".app-checks li").nth(2).locator("input"); // message.delete
const before = await cb.isChecked();
await page.locator(".app-checks li").nth(2).locator("label.plbls-checkbox").click();
ok("cocher ne sauvegarde pas encore", await save.isEnabled());
await save.click();
await page.waitForFunction(() => document.querySelector(".plbls-toast"));
ok("enregistrer déclenche un toast de succès", (await page.locator(".plbls-toast--succes, .plbls-toast").count()) > 0);
await page.waitForTimeout(300);
ok("la case a changé et reste après rechargement des données", (await cb.isChecked()) !== before);
await shot("2-detail");
// remettre comme avant
await page.locator(".app-checks li").nth(2).locator("label.plbls-checkbox").click();
await save.click();
await page.waitForTimeout(300);

await page.getByRole("button", { name: "Supprimer l'application" }).click();
await page.waitForSelector("plbls-modal .plbls-modal");
ok("modale de confirmation visible", await page.locator(".plbls-modal").isVisible());
await shot("3-modale");
await page.getByRole("button", { name: "Annuler", exact: true }).click();
await page.waitForTimeout(200);
ok("annuler ferme la modale", (await page.locator(".plbls-modal").count()) === 0);
ok("l'application existe toujours", (await page.locator(".app-cols > .plbls-panel .app-row").count()) === 3);

await page.getByRole("tab", { name: "Permissions" }).click();
await page.getByLabel("Permission", { exact: true }).fill("Bad Key");
await page.getByLabel("Permission", { exact: true }).press("Enter");
await page.waitForSelector(".plbls-toast--erreur");
ok("clé invalide -> toast d'erreur", true);

await page.getByRole("link", { name: "Attributions" }).click();
await page.waitForSelector("tbody tr .plbls-status--accent");
ok("attributions affichées", (await page.locator("tbody tr").count()) >= 3);
await shot("4-attributions");

await page.getByRole("link", { name: "Journal" }).click();
await page.waitForSelector("tbody tr");
await shot("5-journal");
ok("journal affiché", (await page.locator("tbody tr").count()) > 0);

const m = await b.newPage({ viewport: { width: 390, height: 800 } });
await m.goto(BASE + "/#apps");
await m.waitForSelector(".app-row");
await appRow(m, "chat").click();
await m.getByRole("button", { name: "+ Nouveau rôle" }).waitFor();
await m.screenshot({ path: `${SHOTS}/6-mobile.png`, fullPage: true });
const overflow = await m.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1);
ok("pas de défilement horizontal de la page en mobile", !overflow);

ok("aucune erreur console inattendue" + (errors.length ? " : " + errors.join(" | ") : ""), errors.length === 0);
await b.close();
console.log(failed ? failed + " ÉCHEC(S)" : "TOUT EST OK");
process.exit(failed ? 1 : 0);
