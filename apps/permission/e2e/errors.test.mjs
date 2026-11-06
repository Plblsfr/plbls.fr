// Réponses d'erreur du gateway : corps vide (il ne renvoie que le statut) et statusText vide (HTTP/2).
// Jamais de spinner infini ni de toast vide ; 401 et 403 remplacent l'écran. Interception côté navigateur.
import { chromium } from "playwright";

const BASE = process.env.FRONT_URL ?? "http://127.0.0.1:3000";
const SHOTS = process.env.SHOTS ?? "/tmp/e2e-shots";
let failed = 0;
const ok = (label, cond) => { if (!cond) failed++; console.log((cond ? "ok   " : "FAIL ") + label); };
const b = await chromium.launch();

async function open(hash, handler) {
  const page = await (await b.newContext({ viewport: { width: 1280, height: 800 } })).newPage();
  await page.route("**/api/permission/**", handler);
  await page.goto(`${BASE}/#${hash}`);
  return page;
}
const empty = (status) => (route) => route.fulfill({ status, body: "", headers: {} });

for (const [status, text] of [[404, "Route introuvable"], [502, "injoignable depuis le gateway"], [503, "Authentification indisponible"]]) {
  const page = await open("apps", empty(status));
  await page.waitForTimeout(800);
  const alert = page.locator(".plbls-alert--erreur");
  ok(`${status} sans corps : message d'erreur explicite`, (await alert.count()) > 0 && (await alert.first().innerText()).includes(text));
  ok(`${status} : plus de spinner`, (await page.locator(".plbls-spinner").count()) === 0);
  ok(`${status} : bouton Réessayer`, await page.getByRole("button", { name: "Réessayer" }).isVisible());
  const toasts = await page.locator(".plbls-toast").allInnerTexts();
  ok(`${status} : aucun toast vide`, toasts.every((t) => t.replace("✕", "").trim() !== ""));
  if (status === 502) await page.screenshot({ path: `${SHOTS}/e1-502.png` });
}

for (const hash of ["attributions", "audit"]) {
  const page = await open(hash, empty(502));
  await page.waitForTimeout(800);
  ok(`#${hash} : erreur affichée au lieu d'un spinner infini`, (await page.locator(".plbls-alert--erreur").count()) > 0 && (await page.locator(".plbls-spinner").count()) === 0);
}

{
  const page = await open("apps", (route) => route.abort("failed"));
  await page.waitForTimeout(800);
  ok("panne réseau : message explicite", (await page.locator(".plbls-alert--erreur").first().innerText()).includes("Gateway injoignable"));
}

{
  const page = await open("apps", empty(403));
  await page.waitForSelector(".plbls-empty-state");
  ok("403 : écran « Accès réservé »", await page.getByText("Accès réservé").isVisible());
  const page2 = await open("attributions", empty(403));
  await page2.waitForSelector(".plbls-empty-state");
  ok("403 sur une autre page : écran « Accès réservé »", await page2.getByText("Accès réservé").isVisible());
  const page3 = await open("apps", empty(401));
  await page3.waitForSelector(".plbls-empty-state");
  ok("401 : écran de connexion", await page3.getByText("Session absente ou expirée").isVisible());
}

{ // écriture en échec : toast avec un texte, pas un toast vide
  const page = await open("apps", (route) => route.request().method() === "GET" ? route.continue() : route.fulfill({ status: 502, body: "" }));
  await page.waitForSelector(".app-row");
  await page.getByText("Nouvelle application").click();
  await page.getByLabel("Clé").fill("essai");
  await page.getByLabel("Nom", { exact: true }).fill("Essai");
  await page.getByRole("button", { name: "Créer l'application" }).click();
  await page.waitForSelector(".plbls-toast--erreur");
  const t = (await page.locator(".plbls-toast--erreur").first().innerText()).replace("✕", "").trim();
  ok("écriture 502 : toast non vide (« " + t + " »)", t.length > 0);
}

await b.close();
console.log(failed ? failed + " ÉCHEC(S)" : "TOUT EST OK");
process.exit(failed ? 1 : 0);
