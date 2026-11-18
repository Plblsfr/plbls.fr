CREATE TABLE trophees (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    cle text NOT NULL UNIQUE,
    nom text NOT NULL,
    description text NOT NULL,
    icone text NOT NULL,
    cache boolean NOT NULL DEFAULT false,
    actif boolean NOT NULL DEFAULT true,
    genre text NOT NULL CHECK (genre IN ('paliers', 'exploit')),
    stat text,
    periode text CHECK (periode IN ('jour', 'semaine', 'mois', 'total')),
    seuils bigint[] NOT NULL DEFAULT '{}',
    exploit text,
    parametre bigint,
    ordre integer NOT NULL DEFAULT 0,
    cree_le timestamptz NOT NULL DEFAULT now(),
    modifie_le timestamptz NOT NULL DEFAULT now(),
    CHECK (
        (genre = 'paliers' AND stat IS NOT NULL AND periode IS NOT NULL AND exploit IS NULL
            AND cardinality(seuils) BETWEEN 1 AND 6)
        OR (genre = 'exploit' AND exploit IS NOT NULL AND stat IS NULL AND periode IS NULL
            AND cardinality(seuils) = 0)
    )
);

CREATE TABLE deblocages (
    sub text NOT NULL,
    trophee bigint NOT NULL REFERENCES trophees (id),
    palier integer NOT NULL,
    date_deblocage date NOT NULL,
    vu_le timestamptz,
    PRIMARY KEY (sub, trophee, palier)
);

CREATE INDEX deblocages_trophee ON deblocages (trophee);

CREATE TABLE a_evaluer (
    sub text PRIMARY KEY,
    marque_le timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE recaps (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    sub text NOT NULL,
    genre text NOT NULL CHECK (genre IN ('mois', 'annee')),
    periode text NOT NULL,
    cree_le timestamptz NOT NULL DEFAULT now(),
    contenu jsonb NOT NULL,
    vu_le timestamptz,
    UNIQUE (sub, genre, periode)
);

CREATE TABLE partages_stats (
    sub text NOT NULL,
    stat text NOT NULL,
    PRIMARY KEY (sub, stat)
);

CREATE TABLE partages_trophees (
    sub text NOT NULL,
    trophee bigint NOT NULL REFERENCES trophees (id),
    PRIMARY KEY (sub, trophee)
);

INSERT INTO trophees (ordre, cle, nom, description, icone, cache, genre, stat, periode, seuils) VALUES
    (1, 'cinephile', 'Cinéphile', 'Des films vus jusqu''au bout.', 'film', false, 'paliers', 'entracte.movies_watched', 'total', '{10,50,150,400}'),
    (2, 'encore_un_episode', 'Encore un épisode', 'Des épisodes vus, un par un.', 'tele', false, 'paliers', 'entracte.episodes_watched', 'total', '{50,250,1000,3000}'),
    (3, 'jusqu_au_generique', 'Jusqu''au générique', 'Des séries terminées.', 'clap', false, 'paliers', 'entracte.series_completed', 'total', '{1,5,15,40}'),
    (4, 'heures_de_pellicule', 'Heures de pellicule', 'Du temps passé à regarder.', 'lecture', false, 'paliers', 'entracte.seconds', 'total', '{86400,360000,1800000,5400000}'),
    (5, 'seance_a_plusieurs', 'Séance à plusieurs', 'Des séances au salon cinéma avec du monde.', 'scene', false, 'paliers', 'entracte.cinema_sessions', 'total', '{5,25,100}'),
    (6, 'platine', 'Platine', 'Du temps passé à écouter.', 'casque', false, 'paliers', 'sillon.seconds', 'total', '{86400,360000,1800000,7200000}'),
    (7, 'oreille_curieuse', 'Oreille curieuse', 'Des titres différents écoutés.', 'note', false, 'paliers', 'sillon.distinct_tracks', 'total', '{100,500,2000,5000}'),
    (8, 'en_boucle', 'En boucle', 'Des titres écoutés, répétitions comprises.', 'repeat', false, 'paliers', 'sillon.tracks_played', 'total', '{500,2500,10000,30000}'),
    (9, 'pipelette', 'Pipelette', 'Des messages envoyés.', 'bulle', false, 'paliers', 'messaging.messages', 'total', '{500,2500,10000,50000}'),
    (10, 'bon_public', 'Bon public', 'Des réactions données.', 'sourire', false, 'paliers', 'messaging.reactions_given', 'total', '{100,500,2000,10000}'),
    (11, 'apprecie', 'Apprécié', 'Des réactions reçues.', 'coeur', false, 'paliers', 'messaging.reactions_received', 'total', '{100,500,2000,10000}'),
    (12, 'au_bout_du_fil', 'Au bout du fil', 'Du temps passé en vocal.', 'mic', false, 'paliers', 'messaging.voice_seconds', 'total', '{36000,180000,900000,3600000}'),
    (13, 'tableau_blanc', 'Tableau blanc', 'Du temps passé à partager son écran.', 'ecran', false, 'paliers', 'messaging.screen_share_seconds', 'total', '{7200,36000,180000}'),
    (14, 'pieces_jointes', 'Pièces jointes', 'Des fichiers et des images envoyés.', 'trombone', false, 'paliers', 'messaging.files', 'total', '{25,100,500}'),
    (15, 'habitue', 'Habitué', 'Des jours actifs dans un même mois.', 'calendar', false, 'paliers', 'usage.active_days', 'mois', '{10,20,28}'),
    (16, 'pilier', 'Pilier', 'Du temps actif dans l''app.', 'horloge', false, 'paliers', 'usage.active_seconds', 'total', '{180000,900000,3600000}'),
    (17, 'sur_la_lancee', 'Sur la lancée', 'Des jours d''affilée, toutes activités confondues.', 'etincelle', false, 'paliers', 'streak.global', 'total', '{3,7,30,100,365}'),
    (18, 'seance_quotidienne', 'Séance quotidienne', 'Des jours d''affilée avec une lecture.', 'film', false, 'paliers', 'streak.entracte', 'total', '{3,7,30}'),
    (19, 'un_disque_par_jour', 'Un disque par jour', 'Des jours d''affilée avec une écoute.', 'note', false, 'paliers', 'streak.sillon', 'total', '{3,7,30,100}'),
    (20, 'fidele_au_poste', 'Fidèle au poste', 'Des jours d''affilée avec un message ou du vocal.', 'bulle', false, 'paliers', 'streak.messaging', 'total', '{7,30,100,365}'),
    (29, 'droit_au_but', 'Droit au but', 'Des intros passées.', 'suivant', true, 'paliers', 'entracte.intros_skipped', 'total', '{50,250,1000}'),
    (30, 'zappeur', 'Zappeur', 'Des titres laissés avant la fin.', 'interdit', true, 'paliers', 'entracte.abandoned', 'total', '{10,50}');

INSERT INTO trophees (ordre, cle, nom, description, icone, cache, genre, exploit, parametre) VALUES
    (21, 'premier_jour', 'Premier jour', 'Ta première journée d''activité.', 'etoile', false, 'exploit', 'premier_jour', NULL),
    (22, 'marathon', 'Marathon', 'Cinq épisodes d''affilée.', 'suivant', false, 'exploit', 'episodes_enchaines', 5),
    (23, 'grande_seance', 'Grande séance', 'Une session de quatre heures.', 'horloge', false, 'exploit', 'longue_session', 14400),
    (24, 'journee_canape', 'Journée canapé', 'Huit heures regardées dans la journée.', 'tele', false, 'exploit', 'journee_entracte', 28800),
    (25, 'moulin_a_paroles', 'Moulin à paroles', 'Deux cents messages dans la journée.', 'bulle', false, 'exploit', 'journee_bavarde', 200),
    (26, 'appel_fleuve', 'Appel fleuve', 'Un appel de trois heures.', 'mic', false, 'exploit', 'long_appel', 10800),
    (27, 'journee_bien_remplie', 'Journée bien remplie', 'Six heures actives dans la journée.', 'soleil', false, 'exploit', 'longue_journee', 21600),
    (28, 'signature', 'Signature', 'Le même emoji, encore et encore.', 'sourire', true, 'exploit', 'meme_emoji', 500),
    (31, 'habitant_du_salon', 'Habitant du salon', 'Mille messages dans un même salon.', 'hash', true, 'exploit', 'salon_habite', 1000),
    (32, 'mois_parfait', 'Mois parfait', 'Actif tous les jours d''un mois.', 'calendar', true, 'exploit', 'mois_parfait', NULL);
