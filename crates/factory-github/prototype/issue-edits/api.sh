#!/bin/bash
# Probe P-8: what do lastEditedAt, the edit history and the rename events
# report after a title or body edit made through the API, and who is named?
# Creates one issue on the scratch repository and edits it step by step,
# reading it back after every step.
# Usage: [V=<api-version>] api.sh <output-dir>
set -u
cd "$(dirname "$0")" && . ./lib.sh

OUT=$1
mkdir -p "$OUT"
{ gh --version | head -1; echo "api-version=$V"; date -u +%Y-%m-%dT%H:%M:%SZ; } > "$OUT/00-inputs.txt"

gh label create "$LABEL" -R "$REPO" --color ededed \
  --description "Probe P-8: a label with no meaning to the factory" 2>/dev/null

T0="P-8 probe, API $V: title 0"
B0=$'Body 0.\n\nSecond paragraph.'

rest "$OUT/a00-create.txt" POST issues -f title="$T0" -f body="$B0"
N=$(sed '1,/^$/d' "$OUT/a00-create.txt" | jq -r .number)
ID=$(sed '1,/^$/d' "$OUT/a00-create.txt" | jq -r .node_id)
echo "issue=$N node_id=$ID" >> "$OUT/00-inputs.txt"

# step <name>: wait so that no two steps share a second, then read
step() { sleep 2; observe "$OUT" "$1" "$N"; sleep 2; }

UPDATE='mutation($id: ID!, $title: String, $body: String) {
  updateIssue(input: {id: $id, title: $title, body: $body}) {
    issue { title lastEditedAt updatedAt editor { login } }
  }
}'

step a00-create

# REST, one field at a time, then both
rest "$OUT/a01-rest-title.txt" PATCH "issues/$N" -f title="P-8 probe, API $V: title 1"
step a01-rest-title
rest "$OUT/a02-rest-body.txt" PATCH "issues/$N" -f body=$'Body 2.\n\nSecond paragraph.'
step a02-rest-body
rest "$OUT/a03-rest-both.txt" PATCH "issues/$N" -f title="P-8 probe, API $V: title 3" -f body=$'Body 3.\n\nSecond paragraph.'
step a03-rest-both

# REST, sending the value the issue already has
rest "$OUT/a04-rest-same-title.txt" PATCH "issues/$N" -f title="P-8 probe, API $V: title 3"
step a04-rest-same-title
rest "$OUT/a05-rest-same-body.txt" PATCH "issues/$N" -f body=$'Body 3.\n\nSecond paragraph.'
step a05-rest-same-body

# GraphQL updateIssue, the mutation `gh issue edit` uses
gql "$OUT/a06-graphql-title.txt" "$UPDATE" -f id="$ID" -f title="P-8 probe, API $V: title 6"
step a06-graphql-title
gql "$OUT/a07-graphql-body.txt" "$UPDATE" -f id="$ID" -f body=$'Body 7.\n\nSecond paragraph.'
step a07-graphql-body
gql "$OUT/a08-graphql-same.txt" "$UPDATE" -f id="$ID" -f title="P-8 probe, API $V: title 6" -f body=$'Body 7.\n\nSecond paragraph.'
step a08-graphql-same

# Titles and bodies that differ only in case or white space
rest "$OUT/a09-rest-title-case.txt" PATCH "issues/$N" -f title="P-8 PROBE, API $V: TITLE 6"
step a09-rest-title-case
rest "$OUT/a10-rest-title-trailing-space.txt" PATCH "issues/$N" -f title="P-8 PROBE, API $V: TITLE 6  "
step a10-rest-title-trailing-space
rest "$OUT/a11-rest-body-trailing-newline.txt" PATCH "issues/$N" -f body=$'Body 7.\n\nSecond paragraph.\n'
step a11-rest-body-trailing-newline
rest "$OUT/a12-rest-body-crlf.txt" PATCH "issues/$N" -f body=$'Body 7.\r\n\r\nSecond paragraph.\n'
step a12-rest-body-crlf

# Changes that are not edits of the title or body
rest "$OUT/a13-label.txt" POST "issues/$N/labels" -f "labels[]=$LABEL"
step a13-label
rest "$OUT/a14-close.txt" PATCH "issues/$N" -f state=closed
step a14-close
rest "$OUT/a15-reopen.txt" PATCH "issues/$N" -f state=open
step a15-reopen
rest "$OUT/a16-comment.txt" POST "issues/$N/comments" -f body="A comment, to see whether it moves lastEditedAt."
step a16-comment

# Emptying the body, and putting one back
rest "$OUT/a17-rest-body-empty.txt" PATCH "issues/$N" -f body=""
step a17-rest-body-empty
rest "$OUT/a18-rest-body-back.txt" PATCH "issues/$N" -f body=$'Body 18.'
step a18-rest-body-back

echo "issue $N"
