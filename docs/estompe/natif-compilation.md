# Compilation de l'app native : mesures

Où partent le temps et la mémoire quand on compile et vérifie l'app native (`apps/estompe/natif`), et comment refaire chaque mesure.
Suivi du lot : issue #405. Réglages qui en découlent : [natif.md](natif.md) (« Verrou des builds et mémoire », « Aperçu en direct ») et [ci.md](ci.md).

## Conditions

- Poste de dev : WSL 2 (Ubuntu 26.04), 8 cœurs, 25 Go de mémoire, 7 Go de swap ; toolchain stable 1.99 (clippy 1.100 pour les contrôles de la CI).
- Machine au repos, un seul build à la fois (`scripts/estompe/natif-verrou.sh`).
- `CARGO_TARGET_DIR` sur le disque, jamais sous `/tmp` : sous WSL, `/tmp` est un tmpfs de 13 Go (il prend la mémoire, et une compilation
  qui le remplit ne peut plus écrire).
- Toutes les mesures ci-dessous sont **sans le SDK LiveKit** (`--no-default-features`) : le poste n'a pas clang 21. Les chiffres avec le vocal
  (features par défaut, celles de `natif-check.sh` et de la CI) restent à relever.

## Résultats

### La crate de l'app

| Mesure | Valeur |
|---|---|
| Code Rust généré par Slint (`out/app.rs`) | 77,7 Mo, 274 composants nommés |
| Bibliothèque seule, réglages de la CI (`debug = 0`, sans incrémental) | 228 s, **14,2 Go** de mémoire maximale |
| Bibliothèque seule, profil `dev` local (`debug = "line-tables-only"`) | 219 s ; build complet à froid 4 min 58 s, 17,9 Go |
| Script de build Slint (compilation des `.slint`) | 13,6 s |

Répartition de la compilation de la bibliothèque (`-Z time-passes`, réglages de la CI) : le frontend tient environ 10,6 Go à son terme
(`expand_crate` +2,7 Go, `type_check_crate` +2,0 Go, `MIR_borrow_checking` +3,1 Go, `generate_crate_metadata` +2,0 Go), puis
`codegen_to_LLVM_IR` ajoute 3,9 Go : c'est le pic (14,5 Go). `LLVM_passes` (27 s) rend ensuite la mémoire. Le temps est surtout dans le frontend :
`MIR_borrow_checking` 55 s, `type_check_crate` 37 s, métadonnées 24 s, génération de l'IR 20 s.

Composants les plus lourds (code généré, `node scripts/estompe/natif-poids-ui.mjs`) : la fenêtre racine porte un quart du code, le reste
s'étale sur 273 composants dont aucun ne dépasse 3,2 %.

| Composant | Code généré | Part |
|---|---|---|
| `Principale` | 20,85 Mo | 26,9 % |
| `EditeurRole` | 2,52 Mo | 3,2 % |
| `Contenu` | 1,56 Mo | 2,0 % |
| `FicheCompte` | 1,52 Mo | 2,0 % |
| `Demonstration` | 1,39 Mo | 1,8 % |
| `VueMessages` | 1,37 Mo | 1,8 % |
| `Panneau` | 1,16 Mo | 1,5 % |
| `SectionReplay` | 1,13 Mo | 1,5 % |
| `OngletGroupes` | 1,13 Mo | 1,5 % |
| `OngletNotifications` | 1,10 Mo | 1,4 % |

### Compilations de la crate par vérification

`natif-check.sh` compilait la crate de l'app trois fois en mode build (tests unitaires, bibliothèque pour les tests d'intégration et le
binaire, puis `cargo build` final avec les features hors test), plus deux fois en mode check pour clippy (bibliothèque et tests unitaires).
`cargo test --workspace` lançait les deux premières en même temps : deux rustc de 14 Go sur un runner de 16 Go.

Aujourd'hui : tests unitaires compilés d'abord (`cargo test --workspace --no-run --lib`), puis les tests, qui ne compilent plus que la
bibliothèque ; le `cargo build` final est retiré (il recompilait toute la crate : 246 s, 17,4 Go), le binaire sortant des tests. Reste deux
compilations en mode build, jamais simultanées, et les deux de clippy.

### Autres mesures

| Mesure | Valeur |
|---|---|
| Informations de débogage de Slint coupées | code généré −2,4 % (77,7 → 75,8 Mo), pas de gain de mémoire : piste écartée |
| Aperçu en direct, rechargement d'un `.slint` | 5,4 à 5,9 s, contre 3 min 53 s de recompilation (script de build 13,6 s + crate 219 s) |
| Aperçu en direct, recompilation du Rust seul / mémoire du build | 3,7 s / 2,1 Go |
| Liaison des tests d'intégration après une modification de la bibliothèque | 28 binaires : 165 s ; un binaire regroupé : 4 s |
| `cargo nextest run --workspace` (1 364 tests) | 207 s |

## Refaire les mesures

Depuis `apps/estompe/natif`, avec un dossier cible dédié sur le disque :

```sh
export CARGO_TARGET_DIR="$HOME/.cache/estompe-natif-mesures"
V=../../../scripts/estompe/natif-verrou.sh

# Build à froid : durée, mémoire maximale, répartition par unité (target/cargo-timings/)
/usr/bin/time -v $V cargo build --locked --timings

# Bibliothèque seule, réglages de la CI, répartition par passe (local seulement : RUSTC_BOOTSTRAP)
$V cargo build --locked --lib
touch src/lib.rs
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 RUSTC_BOOTSTRAP=1 /usr/bin/time -v \
  $V cargo rustc --locked --lib -- -Z time-passes 2>&1 | grep -E '^time:|Maximum resident'

# Compilations de la crate pendant les tests (unités estompe-natif du rapport --timings)
touch src/lib.rs
$V cargo test --workspace --locked --no-run --lib
$V cargo test --workspace --locked --no-run --timings

# Poids du code généré, par composant
node ../../../scripts/estompe/natif-poids-ui.mjs "" 10
```

`natif-poids-ui.mjs` prend sans argument le `app.rs` le plus récent de `target/debug/build` ; avec un `CARGO_TARGET_DIR` à part, lui passer le
chemin (`$CARGO_TARGET_DIR/debug/build/estompe-natif-*/out/app.rs`). Ajouter `--no-default-features` à chaque commande pour mesurer sans le vocal,
comme ci-dessus.
