-- Les sondages sont publiés par le compte « Estompe » ; l'utilisateur qui les a lancés est conservé ici.
ALTER TABLE polls ADD COLUMN created_by uuid REFERENCES users(id) ON DELETE CASCADE;
UPDATE polls p SET created_by = m.author_id FROM messages m WHERE m.id = p.message_id;
ALTER TABLE polls ALTER COLUMN created_by SET NOT NULL;
UPDATE messages SET author_id = '00000000-0000-0000-0000-000000000001' WHERE id IN (SELECT message_id FROM polls);
-- Le résultat n'est plus un second message : la carte du sondage est mise à jour.
ALTER TABLE polls DROP COLUMN announced_at;
