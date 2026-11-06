import { createHmac } from "node:crypto";
import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Dev, deux modes pour /api/permission :
//  - défaut : proxy vers le backend local, avec des en-têtes d'identité signés à chaque requête (valables 60 s),
//    comme le ferait auth-gateway. Réglages : DEV_USER_ID, DEV_USER_ROLES ("user" = écran « accès réservé »), DEV_IDENTITY_SECRET.
//  - DEV_VIA_GATEWAY=1 : proxy vers le vrai auth-gateway local (:8080), session par cookie, connexion Keycloak.
const viaGateway = process.env.DEV_VIA_GATEWAY === "1";
const secret = process.env.DEV_IDENTITY_SECRET ?? "dev-identity-secret";
const userId = process.env.DEV_USER_ID ?? "dev-admin";
const roles = process.env.DEV_USER_ROLES ?? "user,permission-admin";
const client = "plbls-gateway";

export default defineConfig({
  plugins: [svelte()],
  resolve: { conditions: ["browser"] },
  test: { environment: "jsdom", setupFiles: ["./src/test-setup.ts"], include: ["src/**/*.test.ts"], css: false, testTimeout: 20000 },
  server: {
    port: 3000,
    proxy: {
      "/api/permission": {
        target: viaGateway ? "http://127.0.0.1:8080" : (process.env.BACKEND_URL ?? "http://127.0.0.1:8090"),
        rewrite: (p) => (viaGateway ? p : p.replace(/^\/api\/permission/, "")),
        configure: (proxy: any) =>
          !viaGateway &&
          proxy.on("proxyReq", (req: any) => {
            const ts = Math.floor(Date.now() / 1000);
            const sig = createHmac("sha256", secret).update(`${userId}\n${roles}\n${client}\n${ts}`).digest("base64url");
            for (const [k, v] of Object.entries({
              "X-User-Id": userId, "X-User-Roles": roles, "X-Client-Id": client, "X-Identity-Ts": String(ts), "X-Identity-Sig": sig,
            })) req.setHeader(k, v);
          }),
      },
    },
  },
});
