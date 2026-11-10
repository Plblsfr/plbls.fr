# Sauvegarde et restauration

La base d'Estompe est la base `estompe` du PostgreSQL partagé (`postgres-main`, stack de `plbls.fr-infra`) : messages, comptes,
serveurs, emojis et avatars, l'essentiel. Elle est sauvegardée hors de la stack Dokploy, avec les autres bases de `postgres-main`.

Les fichiers envoyés sont dans le volume `<projet>_estompe-data` (20 Go au plus, `MAX_STORAGE_GB`) : à sauvegarder aussi, par exemple
`docker run --rm -v <projet>_estompe-data:/data -v "$PWD":/vers alpine tar czf /vers/estompe-data-AAAA-MM-JJ.tar.gz -C /data .`.
Sans eux, un fichier perdu apparaît comme « expiré » dans son message (le même mécanisme que la purge des plus anciens).
Une sauvegarde sur le même disque ne protège pas d'une panne du VPS : la copier ailleurs.

## Restaurer

1. Arrêter l'API pour qu'elle n'écrive plus : `docker compose stop estompe-api`.
2. Recréer la base sur `postgres-main` et la recharger (les données actuelles sont perdues), propriétaire `DATABASE_USER` (`plbls`) :

   ```sh
   docker exec <conteneur postgres-main> dropdb -U postgres estompe
   docker exec <conteneur postgres-main> createdb -U postgres -O plbls estompe
   docker exec -i <conteneur postgres-main> pg_restore -U postgres -d estompe --no-owner --role=plbls < estompe.dump
   ```

   Pour un dump SQL (`pg_dump` sans `-Fc`) : `… psql -U plbls -d estompe -q < estompe.sql` à la place de `pg_restore`.
3. Fichiers : `docker run --rm -v <projet>_estompe-data:/data -v "$PWD":/de alpine tar xzf /de/estompe-data-AAAA-MM-JJ.tar.gz -C /data`.
4. Relancer l'API : `docker compose start estompe-api`. Les migrations manquantes s'appliquent au démarrage.

Une sauvegarde qu'on n'a jamais restaurée n'est pas une sauvegarde : la recharger régulièrement dans une base de test
(`createdb … test_restore`, même commande, `SELECT count(*) FROM messages`, puis `dropdb … test_restore`).
