// Design system @plbls/ui (copie générée par npm run ui:sync à la racine du dépôt) : styles, web components et toasts.
import "./vendor/plbls-ui/plbls-ui.min.css";
import "./vendor/plbls-ui/plbls-ui.esm.min.js";
import { initTheme, setTheme } from "./vendor/plbls-ui/plbls-ui.esm.min.js";
import "./app.css";

// Thème : choix mémorisé, sinon celui du système (le kit est clair par défaut).
initTheme();
if (!document.documentElement.hasAttribute("data-theme")) setTheme("auto", { persist: false });
