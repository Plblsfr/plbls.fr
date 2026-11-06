-- permission gère son propre accès : l'application réservée « permission » porte la permission « admin »
-- (permission:admin) et un rôle du même nom. Qui l'a administre permission, comme le rôle Keycloak (server.admin_role).
WITH app AS (
    INSERT INTO apps (key, name, description)
    VALUES ('permission', 'Permission', 'Administration de permission.plbls.fr (application réservée)')
    RETURNING id
), perm AS (
    INSERT INTO permissions (app_id, key, description)
    SELECT id, 'admin', 'Administrer les applications, permissions, rôles et attributions' FROM app
    RETURNING id, app_id
), role AS (
    INSERT INTO roles (app_id, key, name, description)
    SELECT id, 'admin', 'Administrateur', 'Administre permission' FROM app
    RETURNING id, app_id
)
INSERT INTO role_permissions (role_id, permission_id, app_id)
SELECT role.id, perm.id, perm.app_id FROM role, perm;
