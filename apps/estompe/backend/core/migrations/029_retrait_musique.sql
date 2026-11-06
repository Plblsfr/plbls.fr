-- Retrait du module musique (EST-123) : salons « musique » supprimés (file et messages en cascade),
-- permissions musique (bits 22 à 24 : 29360128) retirées des rôles et des exceptions par salon.
DELETE FROM channels WHERE kind = 'music';
DROP INDEX channels_one_music;
ALTER TABLE channels DROP CONSTRAINT channels_kind_check;
ALTER TABLE channels ADD CONSTRAINT channels_kind_check CHECK (kind IN ('text', 'voice'));
UPDATE roles SET permissions = permissions & ~29360128::bigint WHERE permissions & 29360128 <> 0;
UPDATE channel_overrides SET allow = allow & ~29360128::bigint, deny = deny & ~29360128::bigint WHERE (allow | deny) & 29360128 <> 0;
