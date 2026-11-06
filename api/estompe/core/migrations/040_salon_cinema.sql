-- Salon « Cinéma » (issue 55) : salon vocal dédié aux films de la bibliothèque, lecture synchronisée pilotée par l'hôte.
ALTER TABLE channels DROP CONSTRAINT channels_kind_check;
ALTER TABLE channels ADD CONSTRAINT channels_kind_check CHECK (kind IN ('text', 'voice', 'videos', 'music', 'cinema'));
