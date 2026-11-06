# Sauvegarde et restauration

Le service `backup` de la stack fait un `pg_dump` compressé au démarrage, puis chaque jour à `BACKUP_HOUR` (heure du conteneur, UTC),
dans le volume `estompe-backups`. Les `BACKUP_KEEP` plus récents sont conservés (14 par défaut). Les messages, comptes, serveurs,
emojis et avatars sont dans la base : c'est l'essentiel.

Les fichiers envoyés (volume `estompe-data`, 20 Go au plus) ne sont sauvegardés qu'avec `BACKUP_FILES=true` (une archive par jour).
Sans cela, un fichier perdu apparaît comme « expiré » dans son message (le même mécanisme que la purge des plus anciens).

**Une sauvegarde sur le même disque ne protège pas d'une panne du VPS** : copier régulièrement le volume `estompe-backups` ailleurs
(par exemple `rclone` ou `rsync` depuis l'hôte vers un autre serveur ou un stockage objet).

## Restaurer la base

1. Arrêter l'API pour qu'elle n'écrive plus : `docker compose stop estompe-api`.
2. Choisir une sauvegarde : `docker compose run --rm backup ls -lh /backups`.
3. Recréer la base et la recharger (les données actuelles sont perdues) :

   ```sh
   docker compose exec postgres dropdb -U estompe estompe
   docker compose exec postgres createdb -U estompe estompe
   docker compose run --rm backup sh -c 'gunzip -c /backups/db-AAAA-MM-JJ-HHMM.sql.gz' | docker compose exec -T postgres psql -U estompe -d estompe -q
   ```

4. Si les fichiers sont aussi sauvegardés :
   `docker compose run --rm -v estompe-data:/data backup sh -c 'tar xzf /backups/files-AAAA-MM-JJ.tar.gz -C /data'`
5. Relancer l'API : `docker compose start estompe-api`. Les migrations manquantes s'appliquent au démarrage.

## Vérifier une sauvegarde

Une sauvegarde qu'on n'a jamais restaurée n'est pas une sauvegarde. Tous les mois, la recharger dans une base de test :

```sh
docker compose exec postgres createdb -U estompe test_restore
docker compose run --rm backup sh -c 'gunzip -c /backups/<fichier>.sql.gz' | docker compose exec -T postgres psql -U estompe -d test_restore -q
docker compose exec postgres psql -U estompe -d test_restore -c 'SELECT count(*) FROM messages'
docker compose exec postgres dropdb -U estompe test_restore
```
