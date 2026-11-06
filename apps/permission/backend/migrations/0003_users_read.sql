-- Rechercher des comptes (e-mail) pour les attribuer : permission distincte de permission:admin.
-- Sans elle, l'interface retombe sur la saisie du sub (uuid).
WITH app AS (SELECT id FROM apps WHERE key = 'permission'),
perm AS (
    INSERT INTO permissions (app_id, key, description)
    SELECT id, 'users.read', 'Rechercher des comptes par e-mail' FROM app
    RETURNING id, app_id
), role AS (
    INSERT INTO roles (app_id, key, name, description)
    SELECT id, 'users-reader', 'Lecteur de comptes', 'Recherche des comptes par e-mail' FROM app
    RETURNING id, app_id
)
INSERT INTO role_permissions (role_id, permission_id, app_id)
SELECT role.id, perm.id, perm.app_id FROM role, perm;
