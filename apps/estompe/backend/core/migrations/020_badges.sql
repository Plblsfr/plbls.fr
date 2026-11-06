-- Badges décernés par les administrateurs, affichés dans le profil.
CREATE TABLE badges (
  id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  name        text NOT NULL UNIQUE,
  icon        text NOT NULL,
  color       text NOT NULL,
  description text NOT NULL DEFAULT '',
  created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE user_badges (
  user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  badge_id   uuid NOT NULL REFERENCES badges(id) ON DELETE CASCADE,
  granted_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, badge_id)
);
INSERT INTO badges (name, icon, color, description) VALUES
  ('Fondateur', '👑', '#d4a017', 'A participé à la création d''Estompe'),
  ('Bêta-testeur', '🧪', '#3b82f6', 'A testé Estompe avant son ouverture'),
  ('Contributeur', '🛠️', '#22a06b', 'A contribué à améliorer Estompe');
