// Thème : choix mémorisé, sinon celui du système (le kit est clair par défaut).
import { initTheme, setTheme } from './vendor/plbls-ui/plbls-ui.esm.min.js';
initTheme();
if (!document.documentElement.hasAttribute('data-theme')) setTheme('auto', { persist: false });
