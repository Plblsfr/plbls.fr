# Estompe — frontend

Svelte 5 + Vite + TypeScript, un seul code pour le web, la PWA (mobile) et l'app Windows, Android et iOS (Tauri 2). Thème
`@plbls/ui` embarqué dans `src/vendor/plbls-ui/` : copie générée depuis `packages/ui` par `npm run ui:sync` à la racine du dépôt (voir son README), polices auto-hébergées (`@fontsource-variable`).

## Développement

```sh
nvm use                                      # Node 26, lu dans le .nvmrc de la racine du dépôt
cp .env.example .env                         # VITE_API_URL, VITE_GATEWAY_URL
npm ci --prefix ../..                        # workspace npm : un seul package-lock.json, à la racine du dépôt
npm run dev          # http://localhost:5173
npm run check        # svelte-check (TypeScript 6 : svelte-check n'est pas encore compatible avec TypeScript 7)
npm test             # tests unitaires (vitest) : markdown/XSS, API, WebSocket, store, jetons PKCE…
npm run build
npm run e2e          # bout en bout : vrai navigateur + vrai backend (Docker et Rust requis)
npm run e2e:entracte # sections selon les permissions (Entracte seul, Estompe seul, les deux) : API simulées, sans Docker
npm run e2e:sillon   # section Sillon : permissions, lecture, playlists, file, reprise, ajout, administration, URL à froid : API simulées
npm run e2e:cinema   # mode cinéma du salon cinéma sur un écran de téléphone (portrait et paysage) : API simulées, serveur de dev
npm run e2e:sablier  # section Stats : permissions, périodes, récap, pastille, partage et profil, export, effacement, administration, Sablier arrêté : API simulées
npm run e2e:tirelire # section Tirelire : permission, premier lancement, import, suppression, opérations (recherche, filtres, export CSV), détail, à classer, catégories et règles, accueil, vue du mois et détail d'une catégorie, évolution, écran de téléphone, Tirelire arrêté : API simulées
```

`npm run e2e` compile le backend, lance PostgreSQL dans Docker, un faux auth-gateway qui sert aussi de faux aegis
(`e2e/fake-gateway.mjs` : `PUT /v1/manifest` accepté, `POST /v1/effective` accorde tout le catalogue d'Estompe, sauf à `mallory`,
qui est refusé) et deux utilisateurs dans deux navigateurs (Chromium via Playwright : `npx playwright install chromium`, puis les bibliothèques
système de Chromium doivent être présentes).

## Structure

- `src/lib` : `api` (REST), `realtime` (WebSocket : ticket, reconnexion, ping, renouvellement de session, bandeau de coupure différé), `store.svelte.ts` (état et évènements),
  `markdown` (rendu + DOMPurify), `route.svelte.ts` (routeur sur le hash), `voice.svelte.ts` (LiveKit, chargé à la demande),
  `config.ts` (config d'exécution).
- `src/lib/platform.ts` : interface `Platform` (connexion, mises à jour, notifications, partage natif, raccourci global, espaces, emplacements d'interface…) et objet `platform` que tout le frontend appelle. Le frontend ne teste jamais la plateforme (`isDesktop`, agent, `__TAURI_INTERNALS__`) : chaque plateforme fournit son code. Chaque plateforme (`plateforme/` pour le web, `apps/estompe/windows/` et `apps/estompe/android/` à la racine du dépôt) a un `src/platform.ts` qui exporte son `platform` ; le mode de build choisit lequel (`@platform`, alias Vite) : `vite build` (web), `--mode windows`, `--mode android` (URL de production lues dans `plbls.env` à la racine du dépôt). `src/platform-init.ts`, premier import de `main.ts`, l'installe. Le code partagé par les deux apps Tauri (OAuth PKCE, `native_fetch`, journal debug) est dans `apps/estompe/crates/core/ts/`.
- Un nouveau comportement propre à une plateforme : ajouter une capacité optionnelle à `Platform` (absente = rien à faire), l'implémenter dans le dossier concerné, jamais un `if` dans `src/`. Une page de réglages entière propre à une plateforme passe par `slots.settingsPages` : chaque entrée (`id`, `label`, `icon`, `badge`, `index` pour la recherche, `visible()`, `page`) devient un onglet des paramètres après ceux du cœur, ouvert par `openSettings(id)`. Les tests de `plateforme/`, `apps/estompe/windows/`, `apps/estompe/android/` et `apps/estompe/crates/core/ts/` tournent avec ceux du frontend (`npm test`).
- `src/blocks/<nom>` : un dossier par bloc optionnel (composants, état, tests), avec les noms et dépendances des blocs du backend (`registry.ts`). Le serveur annonce les blocs actifs dans `blocks` de `GET /me` ; `app.on('<nom>')` masque l'interface d'un bloc désactivé ou dont une dépendance l'est. Un nouveau bloc : l'ajouter à `BLOCKS` (le test échoue pour un dossier non déclaré).
- `src/components`, `src/views` : interface du cœur.
- `src/components/acces` (avec `src/lib/acces.svelte.ts`) : administration d'aegis, section « accès » de l'espace d'administration, ouverte au droit `acces` de `GET /me` ; les appels passent par le relais `/admin/acces` du backend.
- `src/sections/entracte` : section Entracte (films et séries, API à part, [`docs/entracte`](../../docs/entracte/README.md)). L'app est
  celle de plbls.fr : la messagerie d'Estompe et Entracte en sont deux sections, chacune affichée selon les permissions. Au démarrage,
  `GET /me` d'Estompe et `GET /me` d'Entracte disent ce que la personne a : Entracte seul ouvre la phase `sections` (écran
  `SectionsShell`, ni WebSocket ni route de la messagerie), les deux affichent l'onglet Entracte dans la messagerie. `filmsApi` lit l'API
  d'Entracte (`entracteUrl` : `ENTRACTE_PUBLIC_URL` de l'image, `VITE_ENTRACTE_URL` au build des apps) ; sans elle, ni section Entracte ni
  salon cinéma. La bibliothèque, la fiche et le lecteur (`Film*.svelte`, `films.ts`) servent aussi au salon cinéma.
  Sans messagerie, la phase `sections` (`views/SectionsShell.svelte`) affiche Entracte, Sillon ou les deux, sans WebSocket ; la section Stats s'y ajoute
  avec `sablier:access`, mais ne suffit pas à ouvrir cette phase.
- `src/sections/regie` : section Régie (administration des médias, API à part, [`docs/regie`](../../docs/regie/README.md)), affichée dans la
  messagerie seulement, selon `GET /me` de Régie (`regie:admin`) ; `regieUrl` : `REGIE_PUBLIC_URL` de l'image, `VITE_REGIE_URL` au build des
  apps ; sans elle, pas de section. `regie.ts` : client de l'API et calculs purs (état d'un service, tri des indexeurs, tailles et débits, états et filtre des torrents) ;
  `regie.svelte.ts` : `Suivi`, qui enchaîne les lectures de `GET /admin/downloads?since=` tant que la page des téléchargements est affichée, et `envoyerFichier`, l'envoi d'un fichier de dépôt par morceaux avec reprise ;
  `matchFiles` (`regie.ts`) associe les fichiers choisis sur l'appareil à ceux du torrent, `sheetText` rédige la fiche technique à copier.
- `src/sections/sillon` : section Sillon (musique, API à part, [`docs/sillon`](../../docs/sillon/README.md)), affichée selon `GET /me` de
  Sillon (`sillon:access`) comme Entracte ; `sillonUrl` : `SILLON_PUBLIC_URL` de l'image, `VITE_SILLON_URL` au build des apps ; sans elle,
  pas de section. `sillon.ts` : client de l'API et calculs purs (formats annoncés d'après `canPlayType`, gain, délai de l'encodeur) ;
  `SillonCover` affiche toute image de Sillon (pochette, portrait, ou image d'un résultat d'ajout par sa référence `ajout`, choisie par
  `vignetteAbsent`), toujours demandée à Sillon avec la session puis rendue en URL `blob:`, vignette de repli sans image ou en échec ;
  `sillon.svelte.ts` : moteur de lecture unique (`Lecteur`, singleton `lecteur`), testable sans DOM (fabrique d'éléments audio, contexte
  Web Audio, Media Session et stockage injectés). Deux éléments `<audio crossorigin="anonymous">` : la piste suivante est demandée et chargée
  dès que la piste en cours joue, puis lancée sur `ended` ; gain de normalisation (piste, ou album quand on écoute un album, plafonné par le
  pic) par un `GainNode` si le contexte audio tourne, sinon par le volume de l'élément (atténuation seule) : un contexte suspendu faute de
  geste ne coupe jamais le son. La file est celle du serveur (`/queue`, injectée par `serveur`) : `lecteur.jouer(pistes, index, contexte)`
  la remplace, `lecteur.elements` donne ses éléments (`suggested`, `discovery`, marque par `marqueDe`), chaque écriture porte la révision
  et un 409 recharge l'état puis rejoue ; chaque changement de piste envoie l'`index` (le serveur y ajoute les suggestions suivantes,
  reçues et préchargées), la position seule part toutes les 15 s de lecture, à la pause et en quittant la page, la répétition
  (`lecteur.repetition`, `changerRepetition`) décide de la suite en fin de titre et de file, et chaque écoute part à l'historique.
  `lecteur.similaires(piste)` joue les titres proches, `lecteur.nePlusProposer(element)` refuse puis retire de la file.
  Au démarrage (`chargerFile`), la file du serveur est proposée à la reprise (`aReprendre`). Seul le volume reste dans le navigateur.
  `aimes` (likes), `collection` (playlists) et `menuPiste` / `menuContenu` (menu contextuel commun : lire ensuite, file, playlist, lien) sont
  dans `sillon.svelte.ts`. Le mini-lecteur (`SillonMini`) est dans le dock de la barre latérale et, sous 900 px,
  en bande au bas de la vue : la lecture continue dans la messagerie et dans Entracte. `lib/medias.ts` : un film d'Entracte qui démarre
  (`FilmPlayer`) ou un salon vocal rejoint (`voice.join`) l'annonce, et Sillon se met en pause.
- `src/sections/sablier` et `src/lib/battement.ts` : temps d'utilisation (Sablier, service de stats à part, [`docs/sablier`](../../docs/sablier/README.md)).
  `sablier.ts` : client de l'API (`me`, `battre`) ; `sablierUrl` : `SABLIER_PUBLIC_URL` de l'image, `VITE_SABLIER_URL` au build des apps ; sans elle,
  aucun battement. Dès qu'une session est ouverte (`sessionOuverte`, quelle que soit la phase : messagerie, sections seules, administration,
  activation, accès refusé) et quelle que soit la section, `App.svelte` lance `suivreUtilisation` : `POST /heartbeat` aussitôt puis toutes les
  `heartbeat_seconds` de `GET /me` de Sablier (30 s par défaut), seulement tant que la page est visible (`visibilityState`) **et** a le focus
  (`document.hasFocus()`). Masquage, perte du focus, `pagehide` ou `freeze` retirent le minuteur aussitôt ; le retour envoie un battement puis
  reprend le cycle. Le battement porte la plateforme (`platform.id()` : `web`, `pwa`, `windows`, `android`) et la section déduite de la route
  (`sectionDe` : `messagerie`, `entracte`, `sillon`, `regie`, `sablier`, `admin`, `autre`). Un échec (réseau, 401, 5xx) est ignoré : ces appels
  passent l'option `discret` de `requestAt`, sans seconde tentative ni perte de session, et le battement suivant part à son heure.
  **Section Stats** (`#/stats[/<sous-section>[/<id>[/<détail>]]]`, route `stats`) : affichée selon `GET /me` de Sablier (`permissions.access`, lu au démarrage
  par `app.loadSablier` : `app.sablierMe`, `app.stats`) ; `/me` en échec ou Sablier absent : pas de section, rien d'autre ne change. Entrée dans le
  rail (`ServerRail`) et dans `SectionsShell`, navigation `SablierNav`, coque `SablierSpace` :
  - `#/stats` : `SablierJeu`, jeu et trophées, de haut en bas : `SablierSerieJours` (série en cours, record, calendrier des quatre
    dernières semaines), `SablierActivite` (grille des douze derniers mois, une case par jour, teinte selon le temps actif),
    `SablierVitrine` (« Presque là » : les trois trophées les plus proches ; « Collection » : une carte par trophée, médaille à la matière
    du palier atteint, bronze, argent, or ou exploit, anneau d'avancement vers le palier suivant), puis les stats par domaine :
    `SablierPeriode` (semaine, mois, année, tout, périodes précédentes) et une `SablierCarte` par domaine. `SablierAnnonce` (trophée
    débloqué ou récap disponible à l'ouverture, dans `Modal`). `SablierSeries` et `SablierTrophees` ne servent plus qu'à `SablierPersonne`.
  - `#/stats/recaps` : `SablierRecaps`, liste des récaps ; `#/stats/recaps/<id>` : `SablierRecap`, le récap en histoire, plein écran
    (flèches, Début, Fin, Échap ; moitiés gauche et droite ; balayage).
  - `#/stats/partage` : `SablierPartage`, réglage de partage stat par stat et trophée par trophée (`PUT /sharing` à chaque bascule), puis
    `SablierDonnees` : export (`GET /export` lu en blob par `fichierAt` de `lib/api.ts`, enregistré sous le nom de `Content-Disposition`
    par un lien `download`, comme les autres exports de l'app), effacement (`DELETE /data`, après la confirmation partagée `ConfirmModal`
    où il faut écrire EFFACER ; l'état de la section est ensuite remis à zéro et la section rouverte), journal « qui a consulté mes
    stats » (`GET /views`, absent s'il est vide).
  - `#/stats/admin[/<onglet>[/<id>]]` : `SablierAdmin`, seulement avec `permissions.admin` (sinon la route rend le jeu et les trophées).
    Onglets `personnes` (`SablierAdminPersonnes` : liste, recherche locale, personnes sans compte, avertissement quand l'annuaire est
    `local`), `personnes/<sub>` (`SablierPersonne` : les mêmes cartes, séries, trophées et récaps que la section, lus par les routes
    `/admin/people/{sub}/…` ; bandeau « consultation notée au journal », absent pour ses propres stats ; salon que l'admin ne voit pas :
    « salon supprimé ou inaccessible »), `totaux` (`SablierAdminTotaux`), `journal` (`SablierAdminJournal`, pages « Plus anciens »),
    `trophees` (`SablierAdminTrophees` : liste, activation), `trophees/nouveau` et `trophees/<id>` (`SablierTropheeEdition`). L'effacement
    des données d'une personne passe par la même confirmation EFFACER (`effacement.ts`).
  - Profil : `SablierProfil` dans `ProfileModal`, si Sablier est configuré et que le lecteur a `sablier:access` ; il rend ce que
    `GET /profiles/{user}` sert pour l'identifiant Estompe de la personne (le sien compris, donc aussi depuis un message privé), et rien
    si c'est vide ou en échec. Le front ne connaît le `sub` d'aucune autre personne.
  - Logique pure, testée à part : `formats.ts` (durées, nombres, jours), `periodes.ts` (bornes et navigation : ni avant `first_day`, ni après
    la période en cours), `stats.ts` (ce qu'une carte affiche : une stat nulle n'a pas de ligne, `{access: false}` donne la carte courte au
    cadenas, un domaine vide une carte qui renvoie vers une période plus large qui a des données ; noms des salons lus dans `app.details`,
    « salon supprimé » sinon ; séries ; trophées ; `collection` et `matiereDe` pour la vitrine ; `grilleActivite` et `quatreSemaines` pour
    les deux calendriers), `recap.ts` (écrans du récap rédigés depuis les totaux et `highlights`, écran sans rien à
    dire sauté ; rangement de la liste), `admin.ts` (noms et filtres des listes, lignes du tableau des trophées, formulaire d'un trophée :
    `versRequete` valide comme le serveur et rend le corps du `POST`, ou les seuls champs changés pour le `PATCH` ; durées saisies en
    heures et envoyées en secondes). `essais.ts` : jeux d'essai des tests.
  - `sablier.ts` : `sablierApi` (routes de la personne), `adminApi` (routes `sablier:admin`), `lecturesDe(sub)` (les lectures d'une
    personne par les routes d'administration, de même forme que celles de `sablierApi`).
  - `sablier.svelte.ts` : `NavigationPeriode` (période, date, bornes ; sert aussi aux totaux) et l'état (`sablier`, classe `EtatSablier`,
    client et horloge injectables ; troisième argument `consultation` pour la vue d'administration : ni annonce, ni `/unseen`, ni
    `POST /seen`). `admin.svelte.ts` : liste des personnes de l'administration (`admin`). `ouvrir` charge trophées, séries, activité par jour
    (`GET /usage?period=year` de l'année en cours, et de la précédente si le premier jour d'activité la précède ; pas en consultation ;
    un échec masque les calendriers sans bloquer la section), stats et non vus ; `choisir`, `reculer`, `avancer`, `aller` rechargent les quatre domaines ; `tropheeVu` et `lireRecap` envoient `POST /seen`
    puis relisent `/unseen` (pastille du rail et de la navigation : `sablier.pastille`, `sablier.nonVus`) ; `chargerRecaps`,
    `chargerPartage`, `partager`. Sablier injoignable : `erreur` et l'écran `SablierErreur` avec « Réessayer », le reste de l'app continue.
  - Styles : `src/styles/sablier.css` (classes `sab-*`, jetons de l'app, requêtes de conteneur). Icônes des trophées : noms d'`Icon.svelte`,
    `trophee` en repli pour un nom inconnu (`SablierMedaille`).
- `src/sections/tirelire` : section Tirelire (relevés de compte, API à part : service `tirelire`, permission `tirelire:access`), `#/tirelire[/<onglet>[/<id>]][?<filtres>]`,
  route `tirelire`. Affichée selon `GET /me` de Tirelire (`permissions.access`, lu au démarrage par `app.loadTirelire` : `app.tirelireMe`, `app.tirelire`) ;
  `/me` en échec ou Tirelire absent : pas de section, rien d'autre ne change. `tirelireUrl` : `TIRELIRE_PUBLIC_URL` de l'image, `VITE_TIRELIRE_URL` au build
  des apps ; sans elle, pas de section. Entrée dans le rail (`ServerRail`) et dans `SectionsShell` ; comme Stats, elle ne suffit pas à ouvrir la phase `sections`.
  - Coque `TirelireSpace` : en-tête (`TirelireTete`), contenu, et barre d'onglets en bas sous 900 px (`TirelireOnglets` : Accueil, Mois, Évolution,
    Habitudes, Opérations) ; au-dessus, les mêmes entrées et les outils (« À classer » avec son compteur, « Catégories et règles », « Import ») sont
    dans le panneau latéral (`TirelireNav`). Sous 900 px, l'en-tête porte une icône « à classer » avec le compteur tant que la file n'est pas vide.
    Les écrans d'outil n'ont pas de barre d'onglets, mais un retour. Cibles tactiles de 44 px au moins.
  - `#/tirelire` : sans relevé, `TirelireVide` (premier lancement) ; sinon `TirelireAccueil`, le tableau de bord de la période la plus récente
    (`GET /stats/month`) : état de la période, bandeau « N opérations à classer » (`GET /unclassified/count`) tant que la file n'est pas vide, anneau
    de budget (ou une invitation à fixer des plafonds s'il n'y en a aucun), projection de fin de mois quand le service en donne une (barre
    dépensé, dépassement, à venir) ou la raison de son absence, tuiles revenus, épargne, reste à vivre et taux d'épargne avec leur courbe
    miniature (`TirelireEtincelle`), « Ce qui dépasse » en cartes classées, bascule € / % (`parRang` suit les rangs calculés par le service),
    catégories sous leur plafond. À partir de 860 px de large (conteneur de la section), deux colonnes et la courbe cumulée.
  - `#/tirelire/mois[/<période>]` : `TirelireMois`. Sélecteur (précédente, suivante, liste des périodes de `GET /stats/months`), bascule « Par
    relevé » / « Mois calendaire » mémorisée (`localStorage`, clé `tirelire.mode`), dépenses et écarts (période précédente, au même jour pour
    une période en cours, moyennes 3 et 12), barre fixe / variable / à classer, courbe cumulée (`TirelireCourbe`) et tableau par catégorie : un
    vrai `<table>`, mis en lignes à deux étages sous 860 px. Chaque catégorie mène à son détail, « À classer » à la file.
  - `#/tirelire/categorie/<id>?periode=<période>` : `TirelireCategorie` (`GET /stats/month/categories/{id}`). Montant et écarts, historique
    en barres avec la ligne du plafond, plafond modifiable sur place (boutons, curseur, repères ; `PATCH /categories/{id}`), commerçants de la
    catégorie sur la période, cinq dernières opérations et lien vers l'écran Opérations filtré (`filtresPeriode` : par relevé, ou par dates avec
    `date=purchase` en mode calendaire, pour retrouver exactement les opérations comptées).
  - Graphiques en SVG et en CSS, sans bibliothèque. `TirelireCourbe` : période (teinte `--tir-c5`), période précédente (`--tir-c1`), moyenne
    (gris), ligne de budget et projection en tirets, légende, valeur de fin étiquetée ; survol, toucher ou flèches du clavier (`role="slider"`)
    déplacent un repère et une bulle qui donne toutes les séries du jour ; résumé en texte et tableau de données équivalent (`<details>`).
    Un dépassement n'est jamais signalé par la couleur seule : flèche ▲ ▼, signe, étiquette ou icône. Tout calcul vient du service ;
    `stats.ts` ne fait que nommer, formater et placer (`graduations`, `abscisse`, `ordonnee`, `trace`, `jauge`, `arcs`).
  - États : une seule période (ni écart ni moyenne, c'est dit), période partiellement couverte, aucune opération classée, aucun plafond,
    période ou catégorie inconnue, statistiques indisponibles (relance). L'état des statistiques (`tirelire.mois`, `tirelire.categorieDuMois`)
    est rechargé après un classement, un import, une suppression ou un changement de catégorie (`tirelire.version`).
  - `#/tirelire/evolution` : `TirelireEvolution` (`GET /stats/history`, `/stats/balance` et `/stats/years`, chargés ensemble par `tirelire.chargerEvolution`).
    Fenêtre 6 / 12 / 24 périodes ou tout (mémorisée, `localStorage`, clé `tirelire.fenetre`) et même bascule « Par relevé » / « Mois calendaire » que la vue
    du mois (`tirelire.mode`). Blocs : barres empilées par catégorie (`TirelireEmpile` : les cinq plus fortes, « Autres », « À classer » en contour, mois
    incomplet hachuré, ligne du budget ; un seul curseur pour toutes les barres, à la souris, au doigt ou au clavier, et un panneau qui détaille la période
    visée), tuiles (moyenne, périodes au-dessus du budget, tendance du total), tableau catégories × périodes avec total, moyenne, plafond et nombre de
    dépassements, tendance par catégorie (mini-courbe `TirelireEtincelle`, sens en toutes lettres, variation chiffrée), solde du compte (`TirelireSolde` :
    tracé en escalier jour par jour, un tracé par suite de jours couverts, trous hachurés jamais reliés, plus bas et plus haut marqués, tableau des jours
    où le solde change) et comparaison d'une année sur l'autre (`TirelireAnnees` : deux courbes mensuelles, même mois, cumul sur les mois communs, tableau
    par catégorie). Tableaux larges : la page ne défile jamais en largeur, c'est la boîte du tableau (`tir-gr-defile`, première colonne figée, atteignable
    au clavier) qui défile ; à 1280 px le tableau de 12 périodes tient en entier. Un montant au-dessus du plafond porte un ▲ et un fond, jamais la couleur
    seule. États dits tels quels : une seule période, pas assez de périodes complètes pour une tendance, aucun plafond, année précédente absente ou
    partielle, jours sans relevé, solde non rapproché. Calculs purs dans `evolution.ts`.
  - `#/tirelire/habitudes` : `TirelireAttente`, « Bientôt disponible », sans aucun chiffre, avec les liens vers ce qui existe.
  - `#/tirelire/a-classer` : `TirelireClasser`. File `GET /unclassified`, un groupe par commerçant : le premier groupe en grand (libellés bruts,
    choix rapide d'une catégorie), « Passer », puis la liste « Ensuite ». Classer fixe la catégorie de tout le commerçant
    (`PUT /merchants/{id}/category`), ou de la seule opération la plus récente si la case de la règle est décochée ou si l'opération n'a pas de
    commerçant (`PUT /operations/{id}/category`, `scope: operation`) ; un bandeau confirme (« Règle créée pour tout le commerçant », nombre
    d'opérations reclassées) et permet d'annuler.
  - `#/tirelire/categories[/regles|/commercants]` : `TirelireCategories`, trois volets. Catégories : nature, plafond mensuel (saisi en euros,
    `centimes`), budget du mois (`budget` : somme des plafonds des dépenses), création, modification, archivage. Règles : mots-clés ou condition de
    type et de section, catégorie, ordre de priorité (monter, descendre), création, modification, suppression ; le nombre d'opérations reclassées
    est affiché. Commerçants : recherche, catégorie de tout un commerçant, regroupement de plusieurs commerçants cochés, séparation d'une variante.
  - Couleur d'une catégorie (`teinte`, `TirelirePastille`) : huit teintes `--tir-c1` à `--tir-c8` de `tirelire.css` (claires et sombres, écart
    vérifié pour les daltonismes) pour les rangs 0 à 7, un point gris au-delà ; le nom est toujours écrit à côté.
  - `#/tirelire/import` : `TirelireImport`. Dépôt ou choix de plusieurs PDF, envoyés à `POST /statements/import` par lots (`lots` : 24 fichiers et
    32 Mo au plus par envoi ; un fichier de plus de 8 Mo est refusé sur place, sans envoi). Une carte par fichier (`TirelireFichier`), refus en tête :
    importé, déjà importé, refusé (motif traduit par `messageRefus` ; pour `checks_failed`, tableau des écarts entre totaux lus et totaux du relevé,
    éléments introuvables, lignes non reconnues avec leur page), ou « non envoyé » quand l'envoi lui-même échoue (401, 403, 413, réseau : `messageErreur`),
    avec « Réessayer ». Liste des relevés importés (période, nombre d'opérations, date d'import) et suppression après la confirmation partagée `ConfirmModal`.
  - `#/tirelire/operations` : `TirelireOperations`. Opérations groupées par jour (`parJour`, total du jour seulement quand le jour est entièrement chargé),
    pages de 50 (« Afficher la suite »). En tête, champ de recherche (libellé, détails, commerçant ; envoyée 300 ms après la dernière frappe,
    tout de suite avec Entrée) et bouton des filtres : période, catégorie ou « à classer », commerçant, montant minimum et maximum (saisis en euros),
    type, relevé, sens (tout, débits, crédits) ; ce sont les paramètres de `GET /operations` (`parametres`). Chaque filtre actif a sa pastille, qui le
    retire seule (`pastilles`), et « Tout effacer » les retire tous. Les filtres sont dans l'adresse (`#/tirelire/operations?q=…&min=…`, mêmes noms que
    l'API ; `filtresDe` écarte une valeur illisible) : `router.remplacer` la met à jour sans ajouter d'entrée à l'historique, un lien ou un rechargement
    les rétablit, et le retour depuis le détail d'une opération les retrouve. Opérations, débits et crédits affichés sont ceux de toute la sélection
    (`total`, `total_debits`, `total_credits` ; à défaut `totauxConnus`). « Exporter en CSV » télécharge `GET /operations/export.csv` avec la
    session (`fichierAt`, puis `enregistrer` : blob et lien de téléchargement, nom repris de `Content-Disposition`), pour les filtres en cours.
    Chaque ligne porte le nom du commerçant et la pastille de sa catégorie, ou « à classer ». `#/tirelire/operations/<id>` : `TirelireOperation`, détail d'une opération, prise dans la liste chargée ou lue par
    `GET /operations/{id}` (libellé brut, détails, dates d'achat, d'opération et de valeur, section, type, relevé), et carte Catégorie : origine du
    classement, choix d'une catégorie, appliqué à cette opération seulement ou à tout le commerçant.
  - `formats.ts` : montants à partir de centimes entiers, sans flottant (`euros` : « 1 234,56 € », espace fine insécable des milliers, espace insécable
    avant le symbole ; `montant` signé selon le sens ; `ecart`), dates `AAAA-MM-JJ` lues en UTC (`jourLong`, `dateCourte`, `dateLongue`, `periode`).
    `tirelire.ts` : `tirelireApi`, types de l'API, messages d'erreur, calculs purs. `tirelire.svelte.ts` : état (`tirelire`, classe `EtatTirelire`, client
    injectable). `essais.ts` : jeux d'essai inventés. Styles : `src/styles/tirelire.css` (classes `tir-*`, jetons de l'app).
- `plateforme/` : plateforme web (son `src/` et l'image nginx). Hors de `web/estompe/` : `apps/estompe/windows/` et `apps/estompe/android/` (coques Tauri, chacune avec son `src-tauri`), `apps/estompe/crates/core` (commandes Rust communes aux deux), `.version` à la racine (version de chaque élément d’Estompe et des services : api, web, windows, android, security, aegis…).
- `plateforme/docker` : image nginx. Une même image sert tous les environnements : `API_URL`, `GATEWAY_URL`, `VOICE_URL`
  (et `KEYCLOAK_ISSUER` pour l'app Windows, `ENTRACTE_PUBLIC_URL` pour Entracte, `SILLON_PUBLIC_URL` pour Sillon, `REGIE_PUBLIC_URL` pour Régie, `SABLIER_PUBLIC_URL` pour Sablier, `TIRELIRE_PUBLIC_URL` pour Tirelire) sont lus au démarrage du conteneur et génèrent `config.js` et la CSP.
  `connect-src` et `media-src` n'autorisent que ces hôtes (API, WebSocket, LiveKit, Entracte, Sillon, Régie, Sablier, Tirelire, gateway) : un fichier vidéo ou un flux HLS
  hébergé ailleurs ne se lit pas dans « Regarder ensemble » ni dans le salon Vidéos. Les apps Windows et Android ont la même règle
  (`tauri.conf.json`) et n'ouvrent dans le navigateur système que les liens `https:`, `mailto:` et `tel:`. Les en-têtes de sécurité
  (`estompe-headers.inc`) s'appliquent aussi à `/updates/` et `/healthz` ; HSTS relève du proxy TLS, cette image ne le pose pas.

## Tests de bout en bout

`npm run e2e` (Docker et Rust requis) démarre PostgreSQL, un vrai LiveKit, le vrai backend et le build du front, puis pilote Chromium.
Le micro et la caméra sont factices (injectés dans la page). Si Chromium ne démarre pas faute de bibliothèques système, lancer un
navigateur dans Docker : `docker run --network host -v ~/.cache/ms-playwright:/ms -e PLAYWRIGHT_BROWSERS_PATH=/ms mcr.microsoft.com/playwright:v1.50.0-noble sh -c "cd /tmp && npm i playwright-core@<version> && npx playwright-core run-server --port 3111 --host 127.0.0.1"`
puis `PW_TEST_CONNECT_WS_ENDPOINT=ws://127.0.0.1:3111/ npm run e2e`.

`npm run e2e:entracte` (`playwright.entracte.config.ts`, `e2e/entracte.spec.ts`) construit le front avec des API fictives et les simule
dans la page : il vérifie les trois profils (Entracte seul, Estompe seul, les deux). `npm run e2e:sillon` (`playwright.sillon.config.ts`,
`e2e/sillon.spec.ts`) fait de même pour Sillon, avec des pistes WAV générées dans le test (le Chromium de Playwright ne lit ni AAC ni MP3 de façon fiable).
`npm run e2e:regie` (`playwright.regie.config.ts`, `e2e/regie.spec.ts`) vérifie la section Régie avec et sans `regie:admin`. `npm run e2e:sablier`
(`playwright.sablier.config.ts`, `e2e/sablier.spec.ts`, faux Sablier et fausse API d'Estompe dans `e2e/faux-sablier.ts`, horloge du navigateur
fixée) vérifie la section Stats : permissions, périodes, récap au clavier et au toucher, pastille, partage et profil, export (fichier
téléchargé comparé à celui du serveur), effacement, parcours d'administration, Sablier arrêté, axe. `npm run e2e:tirelire`
(`playwright.tirelire.config.ts`, `e2e/tirelire.spec.ts`, faux Tirelire dans `e2e/faux-tirelire.ts`) vérifie la section Tirelire : permission, premier lancement, import
(les trois issues, refus au contrôle), suppression, opérations et détail, file à classer (règle, annulation), catégories, règles, fusion et séparation de
commerçants, filtres et reclassement depuis le détail, recherche, filtres dans l'adresse, pastilles, total de la sélection et fichier CSV téléchargé,
accueil, vue du mois, détail d'une catégorie (plafond modifié sur place, bascule par relevé, états limites ; `TIRELIRE_CAPTURES_MOIS=<dossier>` en enregistre les captures),
évolution (graphique, tableau figé qui défile dans sa boîte à 390 px et tient en entier à 1280 px, tendances, solde et ses trous, année sur l'autre, fenêtre et
mode mémorisés, états limites ; `TIRELIRE_CAPTURES_EVOLUTION=<dossier>` en enregistre les captures),
écran de 390 px sans débordement, Tirelire arrêté, axe ; `TIRELIRE_CAPTURES=<dossier>` y enregistre
les captures des écrans de catégorisation, `TIRELIRE_CAPTURES_RECHERCHE=<dossier>` celles de la recherche et des filtres. `CHROMIUM_PATH` désigne un Chromium déjà installé
si la version de Playwright n'a pas le sien. `npm run e2e:cinema` (`playwright.cinema.config.ts`, `e2e/cinema.spec.ts`) vérifie le mode cinéma du salon cinéma
sur un écran de téléphone, en portrait et en paysage : il tourne sur le serveur de dev de Vite, parce que le vocal (LiveKit) n'est pas simulé et que le test
marque le salon comme rejoint en important le module `voice.svelte.ts` dans la page. La CI les lance dans le job `front`.

## Application Windows

Nécessite Rust et, sous Windows, WebView2 (présent sur Windows 11). Configuration figée à la compilation
(modes `windows` et `android`), lue dans `plbls.env` à la racine du dépôt : `API_URL`, `GATEWAY_URL`, **`KEYCLOAK_ISSUER`**, `ENTRACTE_PUBLIC_URL`, `SILLON_PUBLIC_URL`, `REGIE_PUBLIC_URL`, `SABLIER_PUBLIC_URL` et `TIRELIRE_PUBLIC_URL`
(vide = pas de section Entracte, Sillon, Régie ou Tirelire, pas de battement vers Sablier). Une variable `VITE_API_URL`, `VITE_GATEWAY_URL`, `VITE_KEYCLOAK_ISSUER`, `VITE_ENTRACTE_URL`, `VITE_SILLON_URL`, `VITE_REGIE_URL`, `VITE_SABLIER_URL` ou `VITE_TIRELIRE_URL` de l'environnement l'emporte.

```sh
cd ../../apps/estompe/windows && PATH=../../../node_modules/.bin:$PATH tauri build   # installeur NSIS dans apps/estompe/windows/src-tauri/target/release/bundle/nsis (le CLI Tauri est installé par `npm ci` à la racine du dépôt)
```

Connexion : Authorization Code + PKCE avec le client public `plbls-desktop`, dans le navigateur système ; redirection
`http://127.0.0.1:<port>/callback` (serveur loopback local). Le jeton d'accès reste en mémoire ; le refresh token est dans le
Credential Manager de Windows. Les appels API partent en `Authorization: Bearer`. L'origine de la fenêtre est
`https://tauri.localhost` : à ajouter à `ALLOWED_ORIGINS` du backend. Pas de certificat de signature : SmartScreen avertira
(« Informations complémentaires » → « Exécuter quand même »).

### Build de test isolé de la production

`apps/estompe/windows/src-tauri/tauri.test.conf.json` se fusionne à `tauri.conf.json` (`--config`) : identifiant `fr.plbls.estompe.test`, nom « Estompe Test »,
sans mises à jour (plugin et artefacts désactivés). Il s'installe et tourne **à côté** de l'app de production : dossier d'installation
et de données (`%LOCALAPPDATA%\fr.plbls.estompe.test`), entrées du Credential Manager (le service est l'identifiant de l'app) et
raccourcis sont distincts. Pas de schéma d'URL ni de deep link sur Windows (la connexion passe par le serveur loopback, port choisi
par l'OS). Il reste branché sur les mêmes URL (`plbls.env`) : pour viser un autre backend, exporter `VITE_API_URL`,
`VITE_GATEWAY_URL` et `VITE_KEYCLOAK_ISSUER` avant la commande.

```sh
apps/estompe/windows/scripts/build-test-windows.sh <ticket>   # apps/estompe/windows/src-tauri/target/…/bundle/nsis/Estompe Test_<version>_x64-setup.exe, copié dans apps/estompe/windows/dist-test/
```

Lancer l'installeur (installation par utilisateur), puis « Estompe Test » depuis le menu Démarrer. Désinstaller depuis les
Paramètres Windows ; supprimer `%LOCALAPPDATA%\fr.plbls.estompe.test` pour repartir de zéro. L'origine de la fenêtre
(`https://tauri.localhost`) est celle de l'app de production.

### Mises à jour automatiques

L'app vérifie les mises à jour 15 s après le démarrage puis toutes les 6 h, et propose de les installer (installation passive,
sans assistant). La clé publique et l'adresse (`https://estompe.plbls.fr/updates/latest.json`) sont dans `tauri.conf.json` ; la clé
privée est dans `~/.tauri/estompe.key` (hors dépôt, **à sauvegarder** : perdue, plus aucune installation ne peut être mise à jour).

Publier une version : fusionner dans `main`, puis lancer à la main le workflow `estompe-livraison-app` sur `main` (onglet Actions de GitHub, « Run workflow ») ou pousser un tag `estompe-app-vX.Y.Z` (l'API a le sien, `estompe-api-vX.Y.Z` : voir `docs/estompe/AGENTS.md`) ; la fusion seule ne construit plus rien. La CI
(`.github/workflows/estompe-livraison-app.yml`) exécute `apps/estompe/windows/scripts/release-windows.sh`, embarque l'installeur signé et `latest.json` dans l'image
`estompe-web` (servis sur `/updates/`) : le déploiement de l'image suffit. Secret du dépôt requis : `TAURI_SIGNING_PRIVATE_KEY`
(contenu de `~/.tauri/estompe.key`). Les apps ne se mettent à jour que si `version` a augmenté : la CI incrémente `.version` (racine) à chaque fusion dans `main` (`scripts/estompe/bump-version.mjs`) d'après le préfixe de la branche (`feat/…` : minor +1, le reste : patch +1, `release/…` : sans effet) et les dossiers modifiés, et au build `bump-version.mjs --apply` recopie `windows` et `android` dans `tauri.conf.json`, `Cargo.toml` et `Cargo.lock`. Voir `docs/estompe/AGENTS.md`. Pour une version majeure, modifier `.version` à la main. L'utilisateur voit un bouton « Mettre à jour » (barre latérale et Paramètres) ; il n'est forcé que si `api/estompe/core/updates.json` le demande au moment de la release (voir « Mises à jour forcées »).

L'app n'installe que des paquets signés par la clé correspondant à la clé publique du build.

### Mesures de performance

Paramètres › Débogage, mode debug activé : « Démarrer une session », puis les scénarios guidés (repos 5 min, gros salon, défilement automatique
10 s, flux de messages, partie de jeu avec overlays, fenêtre réduite), « Arrêter la session » et « Exporter les mesures » (un JSON et un CSV des échantillons).

- Rust (`apps/estompe/windows/src-tauri/src/mesures.rs`) relève chaque seconde la mémoire privée, le working set, son pic et le temps CPU du processus
  hôte et de **tous** les processus WebView2 (liste fournie par `ICoreWebView2Environment13::GetProcessExtendedInfos`, rafraîchie toutes les 5 s).
  Un processus de rendu est attribué à la fenêtre principale ou à un overlay d'après l'adresse de ses cadres (`#overlay`, `#msg-overlay`).
- Le front (`web/estompe/src/lib/sonde.ts`) mesure les intervalles entre images (seulement pendant les scénarios qui en ont besoin), la latence
  réception WebSocket → affichage, la latence de saisie et le démarrage (lancement du processus → première conversation affichée).
- Hors session, rien ne tourne : pas de fil d'échantillonnage, pas d'écouteur, pas de boucle d'images.
- L'export ne contient ni contenu de message ni identifiant ; rien n'est envoyé automatiquement.

Comparer plusieurs exports (médiane des sessions de chaque version et jeu d'options WebView2, scénario par scénario ; les exports de l'app native,
[docs/estompe/natif-mesures.md](../../docs/estompe/natif-mesures.md), ont leur colonne) :

```sh
node apps/estompe/windows/scripts/synthese-mesures.mjs mesures/ --sortie synthese.md
```

### Options de WebView2 (mémoire)

Au démarrage, avant toute webview, `apps/estompe/windows/src-tauri/src/webview2_options.rs` passe à WebView2, par la variable
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` (même environnement pour la fenêtre principale et les overlays, sinon WebView2 refuse de les créer) :
`--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,SpareRendererForSitePerProcess,BackForwardCache` (les trois premières sont celles de Tauri par défaut,
réunies en une seule liste car seule la dernière `--disable-features` compte), `--renderer-process-limit=1`, `--disable-site-isolation-trials` et `--js-flags=--optimize-for-size`.
Options de l'étude écartées :
- `CalculateNativeWinOcclusion` : sous Windows, c'est elle qui arrête le rendu d'une fenêtre recouverte, par un jeu par exemple ;
- `--disable-extensions` : sans effet, les extensions de WebView2 sont désactivées par défaut ;
- `--disable-component-update` : aucun gain mémoire attendu, et elle peut bloquer la mise à jour de composants du runtime.

Fenêtre principale réduite ou masquée : niveau de mémoire cible bas (`ICoreWebView2_19::MemoryUsageTargetLevel`), rétabli quand elle reprend le focus.
L'interrupteur « Options d'économie de mémoire de WebView2 » (Paramètres › Débogage) écrit `options-webview2` dans `%LOCALAPPDATA%\fr.plbls.estompe` pour
les désactiver au prochain démarrage et comparer les mesures. Une variable `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` déjà définie est laissée telle quelle.
Les options actives figurent dans le journal de debug et dans l'export de mesures.

### Overlay natif (expérimental)

Feature cargo `overlay-natif`, désactivée par défaut : overlay des participants du vocal rendu par Slint, avec son moteur logiciel, dans une fenêtre Win32 transparente,
sans webview. Fonctionnement, rendu comparé et mesures restantes : [docs/estompe/overlay-natif.md](../../docs/estompe/overlay-natif.md).

## Applications Android et iOS (hors stores)

Même code Tauri 2 que l'app Windows, projet Android commité dans `apps/estompe/android/src-tauri/gen/android` (capacités mobiles : `apps/estompe/android/src-tauri/capabilities/mobile.json`). Connexion en PKCE dans le navigateur système ; redirection `fr.plbls.estompe:/callback` (schéma privé, RFC 8252) au lieu du serveur loopback, que le système suspend avec l'app. **Le client Keycloak `plbls-android` (public, PKCE) doit accepter l'URI de redirection `fr.plbls.estompe:/callback`.** La connexion en cours (state, verifier PKCE) est gardée dans le stockage de l'app pendant 10 minutes : si Android recharge la page ou relance l'app pendant que l'utilisateur est dans le navigateur, la page suivante échange le code reçu au lieu de le perdre ; une redirection reçue hors connexion est ignorée, et un échec de l'échange s'affiche sans renvoyer vers Keycloak. Mise à jour intégrée : l'app lit `/updates/android.json` (version et SHA-256 publiés par `release-android.sh`), télécharge l'APK, vérifie sa somme (le natif refuse un APK dont la somme diffère) et ouvre l'installeur Android (permission « installer des applications inconnues » demandée au premier essai) ; la première installation reste manuelle. Session : le refresh token est gardé côté Kotlin (`Session.kt`), chiffré en AES-GCM par une clé du Keystore Android, exclu des sauvegardes (`allowBackup="false"`, `dataExtractionRules`) et jamais renvoyé au front ; l'échange du code, le renouvellement et la déconnexion passent par le canal `window.EstompeSession` (`addWebMessageListener`, injecté dans le seul cadre principal d'origine `https://tauri.localhost`, jamais dans les lecteurs intégrés en iframe), qui ne rend que des jetons d'accès. L'émetteur Keycloak et l'API utilisés par le natif (actions des notifications) sont figés au build (`BuildConfig`, mêmes `VITE_KEYCLOAK_ISSUER` et `VITE_API_URL` que le front : variable d'environnement, sinon `KEYCLOAK_ISSUER` et `API_URL` de `plbls.env`). Sur une WebView sans `addWebMessageListener`, le refresh token reste dans le stockage de la page et les actions des notifications ne sont pas authentifiées.

**Android (APK).** Prérequis : JDK 17, SDK Android + NDK 26, `rustup target add aarch64-linux-android`. Créer une fois la clé de signature, **à sauvegarder** (Android refuse de mettre à jour un APK signé avec une autre clé) :

```sh
keytool -genkeypair -keystore ~/.android/estompe.jks -alias estompe -keyalg RSA -keysize 2048 -validity 10000
ANDROID_KEYSTORE_PASSWORD=… apps/estompe/android/scripts/release-android.sh   # → web/estompe/plateforme/dist-updates/Estompe.apk
```

CI : définir la variable de dépôt `ANDROID_APK=1` et les secrets `ANDROID_KEYSTORE_B64` (`base64 -w0 estompe.jks`) et `ANDROID_KEYSTORE_PASSWORD` ; l'APK est servi sur `/updates/Estompe.apk` (boutons « Télécharger » de la connexion et des Paramètres). Installation : autoriser les sources inconnues sur le téléphone. arm64 uniquement (téléphones récents).

**Push (app fermée) via FCM.** Créer un projet Firebase (gratuit), y ajouter l'app Android `fr.plbls.estompe`, puis :
1. télécharger `google-services.json` dans `apps/estompe/android/src-tauri/gen/android/app/` (ignoré par git ; en CI : secret `GOOGLE_SERVICES_JSON_B64`, `base64 -w0 google-services.json`). Sans lui l'APK se construit, sans push ;
2. générer une clé de compte de service (Paramètres du projet › Comptes de service) et mettre son JSON encodé (`base64 -w0 cle.json`) dans `FCM_SERVICE_ACCOUNT`, variable commune à Estompe et Entracte (`deploy/.env.example`, `secret/commun` de Vault).

Activation : Paramètres › « Notifications push ». Le serveur envoie alors les mentions et messages privés aux destinataires hors ligne ; Google voit le titre et le texte du message passer par FCM.

**iOS.** Non automatisé : il faut un Mac (Xcode) et Apple n'autorise l'installation hors App Store que via un compte gratuit (app valable 7 jours, à re-signer avec AltStore/Sideloadly) ou payant. Sans cela, la PWA reste l'option : Safari › Partager › « Sur l'écran d'accueil ». Pour tenter l'app native sur un Mac : `npm run tauri ios init`, puis `npm run tauri ios build` et signer l'IPA.

## Mises à jour forcées

`api/estompe/core/updates.json` (embarqué par l'API, lu par la CI via `scripts/estompe/update-policy.mjs`) : `mandatory` (la dernière release s'impose), `minVersion` (tout client en dessous doit se mettre à jour, même après plusieurs releases sautées) et `message` (raison affichée), communs, surchargeables dans `platforms.web|android|windows`. La CI les écrit dans `latest.json` (Windows), `android.json` et `web.json` (web et PWA) ; `force` reprend `mandatory` pour les anciens clients, qui ignorent les autres champs. L'app relit le JSON au démarrage, au retour au premier plan et toutes les 30 minutes (`lib/updater.svelte.ts`) : sans forçage, bandeau « Nouvelle version disponible » (« Plus tard » possible) ; avec forçage, écran bloquant « Mise à jour requise » (`ForcedUpdate.svelte`), après 2 minutes de compte à rebours si l'utilisateur est en vocal ou écrit (brouillons déjà enregistrés par le composeur ; le web se recharge alors tout seul). L'API refuse (426) les clients sous `minVersion`, ce qui couvre ceux qui n'ont pas relu le JSON. Remettre `mandatory` à `false` et `minVersion` à `""` une fois la migration terminée (les versions des clients connectés sont dans les journaux de l'API et le tableau de bord d'administration).
