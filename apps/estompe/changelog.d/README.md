# Changelog par PR

Chaque PR ajoute **un fichier** `changelog.d/<sujet>.md` (le même que le nom de la branche sans préfixe, ex. `sondages-multiples.md`) :
un fichier par PR évite les conflits de fusion. La CI échoue sinon ; seules les branches `ci/…`, `docs/…` et `release/…` en sont dispensées.

Contenu : une entrée au format de `CHANGELOG.md`, avec le numéro de l'issue GitHub traitée (`- [x] Titre (#190). Ce qui change. Non fait : … Non vérifié : …`).
Sans issue, l'entrée omet la référence (`- [x] Titre. Ce qui change. …`).
Les fichiers sont regroupés dans `CHANGELOG.md` à la release, puis supprimés.
