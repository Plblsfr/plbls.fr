-- Réglages de notifications de l'utilisateur, communs à tous ses appareils (JSON ; '{}' = réglages par défaut).
ALTER TABLE users ADD COLUMN notify_settings jsonb NOT NULL DEFAULT '{}';
