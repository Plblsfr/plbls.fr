# Changelog par PR

Chaque PR ajoute **un fichier** `changelog.d/<sujet>.md` (le même que le nom de la branche sans préfixe, ex. `sondages-multiples.md`) :
un fichier par PR évite les conflits de fusion. La CI échoue sinon ; seules les branches `ci/…`, `docs/…` et `release/…` en sont dispensées.

Contenu : une entrée au format de `CHANGELOG.md`, avec la clé complète du ticket (jamais `#190` : GitHub en ferait un lien vers une issue ou PR du dépôt) (`- [x] EST-190 Titre. Ce qui change. Non fait : … Non vérifié : …`).
Les fichiers sont regroupés dans `CHANGELOG.md` à la release, puis supprimés.
