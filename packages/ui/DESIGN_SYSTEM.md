# Design System — plbls.fr

Direction artistique **« surligneur »** : blanc cassé, noir, rose pastel en
aplat. Le rose ne décore pas — il surligne un mot, marque l'onglet actif, remplit
le survol. Partout ailleurs, c'est du noir sur crème.

**v2 — la forme s'adoucit, la palette ne bouge pas.** Coins ronds en échelle,
boutons et onglets en pilule, surfaces blanches posées sur le crème par des
ombres en couches plutôt que des filets, halo rose sur les champs actifs, petits
ressorts sur les appuis. Pas de dégradé, pas de verre : le flou ne sert que la
barre de navigation, la seule couche qui flotte. Le geste signature reste le
surlignage, qui se pose maintenant de gauche à droite au chargement.

Le design system est un paquet autonome, **`@plbls/ui`** : jetons en variables CSS,
feuille de styles à classes `plbls-*` et web components. Il ne dépend d'aucun
framework — voir le [README](./README.md). Jetons : [`src/scss/_tokens.scss`](./src/scss/_tokens.scss) ;
composants : [`src/scss/components`](./src/scss/components) et [`src/js/components`](./src/js/components).

## Logo

Un **P majuscule de Bricolage Grotesque (800), sur un pavé carré** aux
coins de `6/100`, suivi du mot « plbls ». Le P est entier, centré dans le
pavé (sommet à 17, hauteur de capitale 66 sur 100), décalé d'un point vers la
droite pour compenser la panse. Les lettres sont **vectorisées** : le logo ne
dépend pas du chargement de la police.

| Fichier (`public/`)                        | Usage                                               |
| ------------------------------------------ | --------------------------------------------------- |
| `logo/plbls-logo-clair.svg`                | En-tête du site, fonds clairs                       |
| `logo/plbls-logo-sombre.svg`               | Fonds sombres (hors site : documents, réseaux…)     |
| `logo/plbls-monogramme-{clair,sombre}.svg` | Le pavé seul, quand le mot n'a pas la place         |
| `favicon.svg`                              | Onglet ; passe en version sombre avec le navigateur |
| `favicon.ico`                              | Repli 16/32/48 px, version claire                   |
| `apple-touch-icon.png`                     | Écran d'accueil mobile, 180 px, carré plein         |

| Thème  | Pavé               | P                  | Mot                 |
| ------ | ------------------ | ------------------ | ------------------- |
| Clair  | `--rose` `#f4c7d5` | `--noir` `#101010` | `--noir`            |
| Sombre | `#2a2624`          | `--rose` `#f4c7d5` | `--creme` `#faf6f3` |

Ne pas recolorer, étirer, ni faire déborder le P du pavé : à 2 près, un
débordement se lit comme une erreur de cadrage, et au-delà la panse fait lire
un « D ».

## Couleurs

| Token           | Valeur    | Rôle                                       |
| --------------- | --------- | ------------------------------------------ |
| `--creme`       | `#faf6f3` | Fond de page                               |
| `--creme-creux` | `#f0eae6` | Fond en creux (encarts, pastilles neutres) |
| `--noir`        | `#101010` | Texte, boutons pleins                      |
| `--gris`        | `#6b6560` | Texte secondaire — 5,35:1 sur crème        |
| `--rose`        | `#f4c7d5` | L'aplat signature                          |
| `--rose-fonce`  | `#c8607f` | **Aplats et bordures uniquement** — 3,6:1  |
| `--rose-texte`  | `#a83355` | **Dès qu'il y a du texte** — 5,98:1        |
| `--trait`       | `#dfd8d3` | Filets et bordures                         |
| `--surface`     | `#fff`    | Panneaux, champs, alertes d'erreur         |

`--rose-fonce` et `--rose-texte` sont deux roses de la même famille. Le premier
n'a pas à respecter de ratio de contraste (fond, bordure), le second si.
**Ne pas les intervertir.**

```css
.message-erreur {
  color: var(--rose-texte); /* du texte → le rose accessible */
  border-color: var(--rose-fonce); /* un trait → le rose d'aplat */
}
```

## Thème sombre

Les mêmes jetons changent de valeur ; aucun composant n'a de variante à écrire.
Le clair reste le défaut.

```html
<html data-theme="dark">  <!-- sombre -->
<html data-theme="auto">  <!-- suit prefers-color-scheme -->
<html>                    <!-- clair (défaut) -->
```

| Token           | Sombre    | Contraste                          |
| --------------- | --------- | ---------------------------------- |
| `--creme`       | `#121212` | Fond de page                       |
| `--creme-creux` | `#1e1e1e` | Fond en creux                      |
| `--noir`        | `#ededed` | Encre — 16,0:1 sur fond            |
| `--gris`        | `#b8b8b8` | 9,4:1 sur fond, 5,8:1 sur `--rose` |
| `--rose`        | `#5b2a3c` | Aplat prune sous l'encre claire — 9,8:1 |
| `--rose-fonce`  | `#e07a9a` | Aplats et bordures — 6,6:1         |
| `--rose-texte`  | `#f2a6bd` | Texte rose — 9,8:1 sur fond        |
| `--trait`       | `#333333` | Filets et bordures                 |
| `--surface`     | `#1a1a1a` | Panneaux, champs                   |

Règles : n'écrire aucune couleur en dur dans un composant (utiliser les jetons,
y compris `--surface` et `--ombre` pour les ombres) ; en JS,
`setTheme('light' | 'dark' | 'auto')` pose l'attribut et mémorise le choix,
`initTheme()` le réapplique au chargement.

## Typographie

- **Bricolage Grotesque** pour les titres (`--font-titre`), en `letter-spacing: -0.03em`.
- **Instrument Sans** pour le texte courant (`--font-texte`), 16px / 1.6.

Les deux se chargent depuis Google Fonts (voir le README) ; sans elles, le kit retombe sur `system-ui`.

La classe `plbls-highlight` enrobe un fragment de titre avec l'aplat rose :

```html
<h1>Tout ce que je <span class="plbls-highlight">bricole</span>.</h1>
```

## Rythme et formes

| Token           | Valeur                   | Usage                                             |
| --------------- | ------------------------ | ------------------------------------------------- |
| `--marge`       | `clamp(20px, 5vw, 72px)` | Gouttière de page                                 |
| `--rayon-s`     | `8px`                    | Éléments denses : items de menu, infobulles       |
| `--rayon`       | `12px`                   | Champs, tuiles, lignes de fichier                 |
| `--rayon-l`     | `20px`                   | Cartes, panneaux, alertes, menus                  |
| `--rayon-xl`    | `28px`                   | Modale, zones de dépôt, états vides               |
| `--rayon-pilule`| `999px`                  | Boutons, onglets, chips, badges, select           |
| `--ombre-1…3`   | couches                  | Posé · menu · modale (contour clair en sombre)    |
| `--halo`        | `0 0 0 4px` rose         | Focus des champs, en plus de la bordure noire     |
| `--ressort`     | `cubic-bezier(.34,1.4,.64,1)` | Apparitions et appuis                        |
| `--espace-1…16` | 4 → 64px                 | Échelle pour les gabarits de pages                |
| `--duree`       | `0.16s`                  | Toutes les transitions                            |
| `--focus`       | `2px solid var(--noir)`  | Anneau de focus, jamais retiré                    |

Les composants du kit posent leurs propres valeurs en pixels ; l'échelle
`--espace-*` sert aux gabarits (`_auth-page.scss`, `_account-pane.scss`).

## Composants

Deux familles, un même balisage de classes `plbls-*` (BEM : `bloc__élément`,
`bloc--variante`, états `is-active`, `is-open`, `is-busy`, `is-over`) :

- **CSS seul** — la classe suffit : `plbls-button`, `plbls-badge`, `plbls-card`,
  `plbls-project-row`, `plbls-tag-stack`, `plbls-highlight`, `plbls-hero`,
  `plbls-section-header`, `plbls-navbar`, `plbls-footer`, `plbls-panel`,
  `plbls-alert`, `plbls-status`, `plbls-spinner`, `plbls-stat-tile`,
  `plbls-empty-state`, `plbls-avatar`, `plbls-progress`, `plbls-file-row`,
  `plbls-field` / `plbls-input` / `plbls-textarea` / `plbls-select`,
  `plbls-checkbox`, `plbls-toggle`, `plbls-chip`, `plbls-table`.
- **Web components** (`dist/plbls-ui.esm.js`) — pour le comportement :
  `<plbls-modal>`, `<plbls-dropdown-menu>`, `<plbls-tabs>`,
  `<plbls-toast-container>` + `toast`, `<plbls-otp-input>`,
  `<plbls-copy-field>`, `<plbls-password-strength>`, `<plbls-pagination>`,
  `<plbls-data-table>`, `<plbls-dropzone>`. Infobulles
  (`data-plbls-tooltip`) et barre latérale repliable (`plbls-sidebar`) sont des
  comportements globaux, sans balise.

Chaque composant a son balisage de référence en tête de son fichier SCSS, et
une démo vivante dans [`docs/index.html`](./docs/index.html).

Équivalences avec l'ancienne version Angular : `<plbls-button variante="contour">`
→ `class="plbls-button plbls-button--contour"` ; `[chargement]` → `is-busy` ;
`[pleineLargeur]` → `--block` ; `plbls-highlight` → `.plbls-highlight` ;
`ToastService` → `toast`.

## Règle : toute pièce d'interface vit dans le kit

Une application ne redessine pas ses commandes : elle utilise `@plbls/ui`.
Quand le kit ne couvre pas un besoin, on **ajoute le composant au kit** puis on
s'en sert. Pour une nouvelle pièce :

1. Un partial `src/scss/components/_<nom>.scss` (classes `plbls-<nom>`), le balisage
   de référence en commentaire de tête, déclaré dans `src/scss/plbls-ui.scss`.
2. Si elle a du comportement, un web component dans `src/js/components/`,
   exporté par `src/js/index.js`.
3. Sa démo dans `docs/index.html`, sa version incrémentée dans `package.json`
   si l'API publique change, une ligne dans `CHANGELOG.md`.

## Accessibilité

La palette respecte AA, les modales piègent le focus, les menus et onglets se
pilotent au clavier, les toasts d'erreur sont annoncés en `assertive`. Cases et
interrupteurs sont de vrais `<input>` : clavier, focus et formulaires natifs.

Le réglage système « réduire les animations » est respecté globalement dans
[`_base.scss`](./src/scss/_base.scss).

## Rendu

`docs/index.html` est la vitrine : `npm run build && npm run serve`, puis
http://localhost:4300/docs/.
