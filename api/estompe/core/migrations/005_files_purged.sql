-- Fichier supprimé du disque pour libérer de la place (le plus ancien d'abord) : les métadonnées restent sur le message.
ALTER TABLE files ADD COLUMN purged_at timestamptz;
CREATE INDEX files_stored ON files (created_at, id) WHERE purged_at IS NULL;
