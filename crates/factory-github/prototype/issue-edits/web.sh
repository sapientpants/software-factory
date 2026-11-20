#!/bin/bash
# Probe P-8, fourth part: the same edits made in the web interface.
# `create` makes the issue through the API; the edits are made by hand or by
# a session driving a browser; `read <step>` records what the API reports
# after each one.
# Usage: web.sh <output-dir> create | read <step>
set -u
cd "$(dirname "$0")" && . ./lib.sh

OUT=$1
mkdir -p "$OUT"

case $2 in
create)
  { gh --version | head -1; echo "api-version=$V"; date -u +%Y-%m-%dT%H:%M:%SZ; } > "$OUT/00-inputs.txt"
  rest "$OUT/w00-create.txt" POST issues -f title="P-8 probe, web: title 0" \
    -f body=$'Body 0, through the API.\n\n- [ ] first box\n- [ ] second box'
  N=$(sed '1,/^$/d' "$OUT/w00-create.txt" | jq -r .number)
  echo "issue=$N" >> "$OUT/00-inputs.txt"
  sleep 2; observe "$OUT" w00-create "$N"
  ;;
read)
  N=$(sed -n 's/^issue=//p' "$OUT/00-inputs.txt")
  date -u +"$3 read at %Y-%m-%dT%H:%M:%SZ" >> "$OUT/00-inputs.txt"
  observe "$OUT" "$3" "$N"
  ;;
esac
