# Direction artistique d'Estompe : plbls-ui v3 « Calque »

Estompe garde ce qui fait plbls.fr (le papier, l'encre, le coup de surligneur) et organise l'interface en trois calques,
un Dock unique et une palette de commandes. Les styles sont posés au-dessus de plbls-ui, sans toucher à `web/estompe/src/vendor/plbls-ui`.

## Feuilles de style

| Fichier (`web/estompe/src/styles/`) | Contenu |
|---|---|
| `v3-jetons.css` | Jetons des thèmes « Papier » (clair) et « Fusain » (sombre), `auto` suit le système ; alias des anciens jetons v2 (`--creme`, `--noir`, `--rayon*`…) |
| `v3-kit.css` | Apparence v3 des composants `plbls-*` et utilitaires préfixés `v-` (`.v-btn`, `.v-ib`, `.v-tag`, `.v-chip`, `.v-menu`, `.v-av`, `.v-parle`…) |
| `gabarit.css` | Rail, panneau de navigation, contenu, marge, Dock, tiroir et barre d'onglets mobiles |
| `messagerie.css`, `vocal.css`, `acces.css`, `parametres.css` | Styles partagés de chaque zone ; le reste est dans le `<style>` des composants |

`src/app.css` ne garde que la base (z-index, `body`, focus, barres de défilement, présence) et quelques classes communes.

## Principes

1. **Trois calques** : navigation (rail de pavés + panneau), contenu, et une marge à onglets (Membres, Épinglés) masquable par Ctrl+. ; sa largeur est mémorisée.
2. **Le rose surligne** l'actif, une mention, une sélection, la voix qui parle ; `--rose-fonce` pour les aplats et traits, `--rose-texte` dès qu'il y a du texte, `--rose-doux` pour le survol.
   Les états (`--ok`, `--attention`, `--danger`, `--info`) ont un fond et une encre, toujours doublés d'une icône.
3. **Un seul Dock** d'encre en bas du panneau : vocal, musique partagée, écoute personnelle, mise à jour, puis ma carte.
4. **Tout passe par Ctrl K** : la palette va à un serveur, un salon, un MP ou un espace, lance une action existante, ou cherche dans les messages avec les filtres.
5. **Lisible sans couleur** : présence par formes (disque, demi-lune, barre, anneau), contraste AA dans les deux thèmes, cibles de 44 px au toucher, focus visible.
6. **Un code, quatre écrans** : sous 1200 px la marge passe par-dessus le contenu, sous 900 px la navigation devient un tiroir avec barre d'onglets et la marge une feuille.

## Typographie, formes, mouvement

- Bricolage Grotesque (titres), Instrument Sans (texte, 15 px), JetBrains Mono (heures, compteurs, codes, raccourcis) : polices auto-hébergées (`@fontsource-variable`).
- Rayons 6, 10, 14, 20, 28 et pilule ; avatars et pavés en carrés adoucis. Élévations `--o1` (posé), `--o2` (menu), `--o3` (fenêtre).
- Icônes : `Icon.svelte`, trait de 1,8 sur une grille de 24, noms français (`epingle`, `repondre`, `micoff`…) ; les anciens noms restent acceptés.
- Mouvement : `--d` (160 ms), `--d-l` (280 ms), `--sortie`, `--ressort` ; uniquement `transform` et `opacity`. Les animations sont coupées par
  `prefers-reduced-motion` ou par le réglage « Réduire les animations » (Apparence, `data-motion="reduit"` sur la racine).
- Gestes : la ligne « nouveaux messages » se trace, la voix qui parle respire (contour rose), le dernier message reçu monte de 6 px.

## Densité

Confortable : messages à 15 px, avatar 36 px. Compacte (`data-density`) : 14 px, avatar masqué, heure et pseudo sur la même ligne.
