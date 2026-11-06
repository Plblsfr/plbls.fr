-- Heure à laquelle chaque utilisateur a vu le dernier message (accusés de lecture).
ALTER TABLE read_states ADD COLUMN read_at timestamptz NOT NULL DEFAULT now();
