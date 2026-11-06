# Changelog

## 2.0.1

Livraison seulement : styles et JS inchangés.

- `plbls-ui.min.css` n'a plus de BOM après la bannière (Sass le plaçait devant `:root{` et la règle des jetons était ignorée).
- Nouveaux fichiers : `tokens.min.css` (jetons seuls, minifiés), `plbls-ui.esm.d.ts` et `plbls-ui.esm.min.d.ts` (types du module).
- `package.json` : `types`, exports `./tokens.min.css` et condition `types`.

## 2.0.0

Refonte visuelle. Palette, noms de classes, balisage et JS inchangés ; seule
l'apparence change.

- Coins ronds : échelle `--rayon-s/-/-l/-xl/-pilule` (`--rayon` passe de 2px à 12px).
- Élévations `--ombre-1…3` et halo de focus `--halo` ; surfaces blanches sans
  filet, en sombre un contour clair remplace l'ombre.
- Boutons, onglets (contrôle segmenté), chips, badges, statuts et select en pilule.
- Barre de navigation collante, flou limité à elle (repli opaque avec
  `prefers-reduced-transparency`), lien actif surligné en rose.
- Alertes sans liséré latéral, toasts avec point de ton, cases et interrupteurs
  plus grands (cibles ≥ 22 px), modale à fond flouté.
- Surlignage animé au chargement ; mouvements courts, coupés par
  `prefers-reduced-motion`.
- Rupture visuelle : les gabarits qui comptaient sur des angles à 2px sont à revoir.

## 1.0.0

Thème sombre. Le thème clair reste le défaut et ne change pas.

- Thème sombre par jetons : `data-theme="dark"` sur `<html>`, ou
  `data-theme="auto"` pour suivre `prefers-color-scheme`.
- Nouveaux jetons `--surface` (fond des panneaux et champs) et `--ombre`
  (couleur des ombres), plus `color-scheme` posé par thème.
- Les couleurs en dur des composants (`#fff`, ombres, coche des cases,
  flèche du select) passent par les jetons.
- JS : `getTheme`, `setTheme`, `initTheme` et l'événement `plbls-theme`.
- Vitrine : sélecteur de thème (clair, sombre, système), choix mémorisé.

## 0.3.0

Premier paquet autonome, extrait de `projects/plbls-ui` (Angular, 0.2.0) de
`plbls-fr-web`. Plus aucune dépendance à Angular : feuille CSS à classes
`plbls-*` (BEM), jetons en variables CSS, web components en JS vanilla,
build sass + esbuild, vitrine `docs/index.html`.
