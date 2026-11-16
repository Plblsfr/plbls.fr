CREATE TABLE playlists (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    sub text NOT NULL,
    nom text NOT NULL,
    description text NOT NULL DEFAULT '',
    cree_le timestamptz NOT NULL DEFAULT now(),
    modifie_le timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX playlists_sub ON playlists (sub, modifie_le DESC);

CREATE TABLE playlists_pistes (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    playlist_id uuid NOT NULL REFERENCES playlists (id) ON DELETE CASCADE,
    piste_id uuid NOT NULL REFERENCES pistes (id),
    position integer NOT NULL CHECK (position >= 0),
    ajoute_le timestamptz NOT NULL DEFAULT now(),
    UNIQUE (playlist_id, position) DEFERRABLE INITIALLY DEFERRED
);
CREATE INDEX playlists_pistes_piste ON playlists_pistes (piste_id);

CREATE TABLE likes (
    sub text NOT NULL,
    type text NOT NULL CHECK (type IN ('track', 'album', 'artist')),
    cible uuid NOT NULL,
    cree_le timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (sub, type, cible)
);
CREATE INDEX likes_recents ON likes (sub, type, cree_le DESC);

CREATE TABLE files (
    sub text PRIMARY KEY,
    revision bigint NOT NULL DEFAULT 0,
    source_type text CHECK (source_type IN ('album', 'playlist', 'artist', 'likes', 'search')),
    source_id uuid,
    source_nom text,
    index_courant integer NOT NULL DEFAULT 0 CHECK (index_courant >= 0),
    position double precision NOT NULL DEFAULT 0 CHECK (position >= 0),
    aleatoire boolean NOT NULL DEFAULT false,
    repetition text NOT NULL DEFAULT 'off' CHECK (repetition IN ('off', 'all', 'one')),
    graine bigint NOT NULL DEFAULT 0,
    suggestions text NOT NULL DEFAULT 'off' CHECK (suggestions IN ('off', 'light', 'frequent')),
    modifie_le timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE files_elements (
    sub text NOT NULL REFERENCES files (sub) ON DELETE CASCADE,
    rang integer NOT NULL,
    id uuid NOT NULL,
    piste_id uuid NOT NULL REFERENCES pistes (id),
    rang_source integer NOT NULL,
    suggeree boolean NOT NULL DEFAULT false,
    PRIMARY KEY (sub, rang),
    UNIQUE (sub, id)
);
CREATE INDEX files_elements_piste ON files_elements (piste_id);

CREATE TABLE ecoutes (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    sub text NOT NULL,
    piste_id uuid NOT NULL REFERENCES pistes (id),
    debut timestamptz NOT NULL,
    duree_ecoutee double precision NOT NULL CHECK (duree_ecoutee >= 0),
    duree_piste double precision,
    fin text NOT NULL CHECK (fin IN ('completed', 'skipped', 'interrupted')),
    compte boolean NOT NULL,
    source_type text CHECK (source_type IN ('album', 'playlist', 'artist', 'likes', 'search')),
    source_id uuid,
    suggeree boolean NOT NULL DEFAULT false,
    cree_le timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX ecoutes_sub ON ecoutes (sub, debut DESC, id DESC);
CREATE INDEX ecoutes_piste ON ecoutes (piste_id);
