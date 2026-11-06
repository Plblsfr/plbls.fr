// Thème : « light » (défaut), « dark » ou « auto » (suit le système).
// Pose l'attribut data-theme sur <html> et mémorise le choix.
const KEY = 'plbls-theme';
const THEMES = ['light', 'dark', 'auto'];

export function getTheme() {
  const attr = document.documentElement.getAttribute('data-theme');
  return THEMES.includes(attr) ? attr : 'light';
}

export function setTheme(theme, { persist = true } = {}) {
  if (!THEMES.includes(theme)) throw new RangeError(`Thème inconnu : ${theme}`);
  document.documentElement.setAttribute('data-theme', theme);
  if (persist) {
    try {
      localStorage.setItem(KEY, theme);
    } catch {
      /* stockage indisponible : le choix vaut pour la page seulement */
    }
  }
  document.dispatchEvent(new CustomEvent('plbls-theme', { detail: theme }));
}

// Applique le choix mémorisé (à appeler tôt pour éviter un flash de thème).
export function initTheme() {
  try {
    const saved = localStorage.getItem(KEY);
    if (THEMES.includes(saved)) setTheme(saved, { persist: false });
  } catch {
    /* ignore */
  }
}
