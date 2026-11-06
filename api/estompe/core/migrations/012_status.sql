-- Statut choisi par l'utilisateur : en ligne, absent ou ne pas déranger.
ALTER TABLE users ADD COLUMN status text NOT NULL DEFAULT 'online' CHECK (status IN ('online', 'idle', 'dnd'));
