#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mapfile -t files < <(git ls-files '*.sh')
status=0
for f in "${files[@]}"; do
  shebang=$(head -n 1 "$f")
  case "$shebang" in
    '#!/usr/bin/env bash') strict='set -euo pipefail' ;;
    '#!/bin/sh') strict='set -eu' ;;
    *) echo "$f : shebang « $shebang » au lieu de #!/usr/bin/env bash (ou #!/bin/sh pour une image Alpine)" >&2; status=1; continue ;;
  esac
  if ! grep -qx "$strict" "$f"; then
    echo "$f : ligne « $strict » absente" >&2
    status=1
  fi
done
shellcheck "${files[@]}" || status=1
exit "$status"
