# Direction artistique d'Estompe (proposition, EST-145)

Statut : **proposition à valider**. Rien n'est appliqué aux écrans tant que la planche n'est pas approuvée.
Contrainte (`AGENTS.md`) : pas de refonte visuelle des composants avant la v2 de plbls-ui. Tout ce qui suit se pose
**au-dessus** de plbls-ui (jetons `--rose`, `--creme`, `--rayon`, `--duree`… et `src/app.css`), sans toucher à `src/vendor/plbls-ui`.

## 1. Concept : « l'estompe »

Une estompe est le rouleau de papier qui fond un trait de fusain. Estompe, l'app, **adoucit** : rien ne claque, tout se fond.
Trois principes :

1. **Le trait se dessine** : un élément qui apparaît se trace (largeur ou `scaleX` d'un filet rose), il ne « pop » pas.
2. **L'actif se surligne** : l'état actif/sélectionné est un coup de surligneur rose (déjà la signature de plbls.fr), pas une pastille ni une barre.
3. **L'arrière-plan se fond** : ce qui n'est pas au premier plan est adouci (opacité, flou léger sur les surfaces qui passent derrière un panneau), jamais masqué net.

## 2. Planche de référence

| Axe | Direction |
|---|---|
| Palette | Existante : crème (fond), noir fusain (texte), rose surligneur (actif), gris (secondaire). Le rose n'est utilisé que pour l'**actif** et le **surlignage** ; pas de couleur d'accent supplémentaire. États `--ok/--attention/--danger` inchangés |
| Typographie | Bricolage Grotesque (titres, serré, `letter-spacing: -0.04em`), Instrument Sans (texte). Inchangé |
| Formes | Rayons doux (`--rayon`, `--rayon-l`, pilule). Les serveurs ne sont **plus des ronds** mais des carrés aux angles adoucis (« papier ») ; la forme ne change pas au survol |
| Ombres | Une seule élévation douce (`--ombre-1`) pour le premier plan ; pas de bordures dures, des filets `--trait` |
| Icônes | Jeu `Icon.svelte` actuel, trait fin, couleur gris → noir ; l'actif passe au rose texte |
| Texture | Aucune image : un dégradé radial rose flou (déjà utilisé sur l'écran d'accueil) sert de « halo d'estompe » derrière l'élément actif ou les écrans vides |

Écrans clés à redessiner (maquettes à fournir pour validation) : fil de messages, salon vocal, profil, navigation.

## 3. Système d'animations

Jetons à ajouter dans `app.css` (réutilisés par tous les composants Estompe) :

| Jeton | Valeur | Usage |
|---|---|---|
| `--mv-rapide` | 120 ms | survol, pressé |
| `--mv-normal` | 200 ms | changement d'onglet, apparition/disparition |
| `--mv-lent` | 320 ms | ouverture de modal, changement de serveur |
| `--mv-sortie` | `cubic-bezier(0.2, 0, 0, 1)` | entrée (décélération) |
| `--mv-entree` | `cubic-bezier(0.4, 0, 1, 1)` | sortie (accélération) |

Effets réutilisables (classes CSS) : `fondu` (opacité + léger flou), `trace` (filet rose qui se dessine, `scaleX` 0 → 1), `surligne`
(fond rose qui s'étend de gauche à droite), `monte` (translation de 6 px + fondu pour l'arrivée d'un message).

Règles :
- uniquement `transform` et `opacity` (le flou sur surfaces fixes seulement, jamais sur une liste) ; rien ne déclenche de mise en page ;
- listes longues : seul le dernier message reçu s'anime, jamais le chargement de l'historique ;
- `@media (prefers-reduced-motion: reduce)` : les jetons passent à `0.01ms` via une seule règle sur `:root`, ce qui couvre tout (aujourd'hui seuls 3 blocs le gèrent) ;
- le réglage « réduire les animations » de l'app (le cas échéant) force la même règle.

## 4. Micro-interactions signature

| Moment | Effet proposé |
|---|---|
| Envoi d'un message | le message se pose avec `monte` ; le trait rose du composeur se « vide » de gauche à droite |
| Réaction | l'emoji s'estompe vers la pastille, compteur qui fond d'un chiffre à l'autre |
| Arrivée en vocal | la vignette apparaît avec `fondu` depuis un halo rose |
| Prise de parole | contour rose qui respire (opacité), remplace le cercle vert vif |
| Changement de serveur / salon | le panneau sortant s'estompe (`fondu`) pendant que le suivant se trace ; le filet actif glisse d'un élément à l'autre |

## 5. Éléments calqués sur Discord et alternatives

| Aujourd'hui (fichier) | Ressemblance | Alternative Estompe |
|---|---|---|
| Rail des serveurs : icônes rondes, pastille d'état à gauche, rond → carré au survol (`ServerRail.svelte`, `.rail-pill`) | identique à Discord | Carrés adoucis fixes ; l'actif = surlignage rose derrière l'icône ; non-lus = filet rose sous l'icône ; pas de transformation de forme |
| Profil : carte avec bannière + pastille de statut (`ProfileModal.svelte`, `.pc-*`) | popover de profil Discord | Fiche « papier » : nom en grand (Bricolage), statut en texte surligné, bannière estompée en dégradé vers la crème |
| Barre d'actions au survol d'un message (`MessageItem.svelte`) | barre flottante Discord | Actions qui se déploient dans la marge du message (trait rose + icônes), accessibles au clavier et visibles en permanence sur mobile |
| Liste des membres groupée par rôle avec point de présence (`MembersPanel.svelte`) | liste Discord | Présence en anneau/trait (déjà non basée sur la couleur seule), groupes séparés par un filet rose fin |
| Panneau utilisateur en bas à gauche (`.me-bar`) | idem Discord | Carte « papier » intégrée à l'en-tête du panneau latéral |
| Salon vocal : liste de participants sous le salon, cercle vert de parole (`SidePanel.svelte`, `VoiceChannelView.svelte`) | idem Discord | Contour rose qui respire ; participants en vignettes qui se fondent dans l'arrière-plan |
| Réactions en pastilles (`MessageItem.svelte`) | idem Discord | Pastilles surlignées au rose quand l'on a réagi (déjà proche), animation d'estompe sur le compteur |

## 6. Tickets de refonte à adapter

Après validation : EST-124, EST-125, EST-126, EST-138 (références à Discord à remplacer par ce document), EST-112 (barres de défilement)
et EST-144 (chargement) reprennent les jetons et effets de la section 3.

## 7. Suite proposée (après validation)

1. Maquettes des 4 écrans clés ; validation.
2. Jetons d'animation et `prefers-reduced-motion` global dans `app.css` (+ test `msg-tools-css.test.ts`-style sur les jetons).
3. Page de démonstration du kit (route réservée au développement).
4. Application écran par écran, une PR par écran.
5. Tests : web PC, Windows, Android, PWA, thèmes clair et sombre.
