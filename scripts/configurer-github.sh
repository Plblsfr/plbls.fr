#!/usr/bin/env bash
set -euo pipefail

depot="${1:-Plblsfr/plbls.fr}"
api() { gh api -H "X-GitHub-Api-Version: 2022-11-28" "$@"; }
etape() { printf '\n== %s\n' "$1"; }

gh auth status >/dev/null

etape "Fusion : merge classique seulement, message par défaut (le nom de la branche y figure)"
api -X PATCH "repos/$depot" --silent --input - <<'EOF'
{
  "allow_merge_commit": true,
  "allow_squash_merge": false,
  "allow_rebase_merge": false,
  "merge_commit_title": "MERGE_MESSAGE",
  "merge_commit_message": "PR_TITLE",
  "allow_update_branch": true,
  "delete_branch_on_merge": true,
  "allow_auto_merge": false,
  "has_wiki": false,
  "has_projects": false,
  "has_discussions": false,
  "web_commit_signoff_required": false
}
EOF

etape "Sécurité : signalement privé, alertes et correctifs Dependabot, secret scanning, push protection"
api -X PUT "repos/$depot/private-vulnerability-reporting" --silent
api -X PUT "repos/$depot/vulnerability-alerts" --silent
api -X PUT "repos/$depot/automated-security-fixes" --silent
api -X PATCH "repos/$depot" --silent --input - <<'EOF'
{
  "security_and_analysis": {
    "secret_scanning": { "status": "enabled" },
    "secret_scanning_push_protection": { "status": "enabled" }
  }
}
EOF

etape "CodeQL (default setup)"
api -X PATCH "repos/$depot/code-scanning/default-setup" --silent -f state=configured -f query_suite=default \
  || echo "CodeQL non configuré : l'activer dans Settings > Code security"

etape "Releases immuables"
api -X PUT "repos/$depot/immutable-releases" --silent \
  || echo "Releases immuables non activées : Settings > General > Releases"

etape "Actions : jeton en lecture seule, pas de PR créée ni approuvée par GITHUB_TOKEN"
api -X PUT "repos/$depot/actions/permissions/workflow" --silent --input - <<'EOF'
{ "default_workflow_permissions": "read", "can_approve_pull_request_reviews": false }
EOF

etape "Actions : approbation exigée pour tout contributeur externe"
api -X PUT "repos/$depot/actions/permissions/fork-pr-contributor-approval" --silent -f approval_policy=all_external_contributors

etape "Actions : actions autorisées et épinglage par SHA obligatoire"
api -X PUT "repos/$depot/actions/permissions" --silent --input - <<'EOF'
{ "enabled": true, "allowed_actions": "selected", "sha_pinning_required": true }
EOF
api -X PUT "repos/$depot/actions/permissions/selected-actions" --silent --input - <<'EOF'
{
  "github_owned_allowed": true,
  "verified_allowed": true,
  "patterns_allowed": ["docker/*", "dtolnay/rust-toolchain@*", "android-actions/setup-android@*"]
}
EOF

etape "Environnement livraison : main et tags *-v* seulement"
api -X PUT "repos/$depot/environments/livraison" --silent --input - <<'EOF'
{ "deployment_branch_policy": { "protected_branches": false, "custom_branch_policies": true } }
EOF
existantes="$(api "repos/$depot/environments/livraison/deployment-branch-policies" --jq '.branch_policies[] | "\(.type):\(.name)"')"
for politique in branch:main tag:'*-v*'; do
  grep -qxF "$politique" <<<"$existantes" && continue
  api -X POST "repos/$depot/environments/livraison/deployment-branch-policies" --silent \
    -f type="${politique%%:*}" -f name="${politique#*:}"
done

ruleset() {
  local nom="$1" id
  id="$(api "repos/$depot/rulesets" --jq ".[] | select(.name == \"$nom\") | .id")"
  if [ -n "$id" ]; then
    api -X PUT "repos/$depot/rulesets/$id" --silent --input -
    echo "ruleset « $nom » mis à jour"
  else
    api -X POST "repos/$depot/rulesets" --silent --input -
    echo "ruleset « $nom » créé"
  fi
}

etape "Ruleset main : PR obligatoire, merge seulement, contrôles requis, ni suppression ni force-push"
ruleset main <<'EOF'
{
  "name": "main",
  "target": "branch",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
  "bypass_actors": [{ "actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "pull_request" }],
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": false,
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": true,
        "allowed_merge_methods": ["merge"]
      }
    },
    {
      "type": "required_status_checks",
      "parameters": {
        "strict_required_status_checks_policy": true,
        "do_not_enforce_on_create": false,
        "required_status_checks": [
          { "context": "npm", "integration_id": 15368 },
          { "context": "changelog", "integration_id": 15368 },
          { "context": "conventions", "integration_id": 15368 },
          { "context": "estompe-backend-rs", "integration_id": 15368 },
          { "context": "estompe-frontend", "integration_id": 15368 },
          { "context": "pli-ci", "integration_id": 15368 },
          { "context": "permission-ci", "integration_id": 15368 },
          { "context": "security-ci", "integration_id": 15368 },
          { "context": "ui-ci", "integration_id": 15368 }
        ]
      }
    }
  ]
}
EOF

etape "Ruleset des tags de livraison : création, déplacement et suppression réservés aux admins"
ruleset "tags de livraison" <<'EOF'
{
  "name": "tags de livraison",
  "target": "tag",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["refs/tags/*-v*"], "exclude": [] } },
  "bypass_actors": [{ "actor_id": 5, "actor_type": "RepositoryRole", "bypass_mode": "always" }],
  "rules": [{ "type": "creation" }, { "type": "update" }, { "type": "deletion" }, { "type": "non_fast_forward" }]
}
EOF

etape "Fait. Reste à la main (Settings > Environments > livraison) : y déplacer les secrets de livraison"
echo "TAURI_SIGNING_PRIVATE_KEY ANDROID_KEYSTORE_B64 ANDROID_KEYSTORE_PASSWORD GOOGLE_SERVICES_JSON_B64 DOKPLOY_WEBHOOK_URL"
