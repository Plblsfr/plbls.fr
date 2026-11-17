ALTER TABLE files ADD COLUMN sans_fin boolean NOT NULL DEFAULT true;

ALTER TABLE files DROP CONSTRAINT files_source_type_check;
ALTER TABLE files ADD CONSTRAINT files_source_type_check
    CHECK (source_type IN ('album', 'playlist', 'artist', 'likes', 'search', 'library'));

ALTER TABLE ecoutes DROP CONSTRAINT ecoutes_source_type_check;
ALTER TABLE ecoutes ADD CONSTRAINT ecoutes_source_type_check
    CHECK (source_type IN ('album', 'playlist', 'artist', 'likes', 'search', 'library'));
