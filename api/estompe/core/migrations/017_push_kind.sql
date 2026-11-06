-- Type d'appareil : web (Web Push), fcm (app Android), desktop (app Windows).
ALTER TABLE push_subscriptions ADD COLUMN kind text NOT NULL DEFAULT 'web' CHECK (kind IN ('web', 'fcm', 'desktop'));
UPDATE push_subscriptions SET kind = 'fcm' WHERE endpoint LIKE 'fcm:%';
