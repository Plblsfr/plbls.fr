-- Couleur de bannière choisie par la personne pour sa carte de profil (« #rrggbb », vide : couleur du rôle le plus élevé).
ALTER TABLE users ADD COLUMN banner_color text NOT NULL DEFAULT '';
