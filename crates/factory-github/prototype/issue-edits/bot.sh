#!/bin/bash
# Probe P-8, third part: who is named when an App edits an issue?
# `start` creates an issue as the operator and pushes probe-p8.yml to the
# branch probe/p-8 of the scratch repository, which runs it. The workflow
# edits that issue, and one of its own, as github-actions[bot].
# `finish`, once the run is over, reads both issues, edits both as the
# operator and reads them again.
# Usage: bot.sh <output-dir> start|finish
set -u
cd "$(dirname "$0")" && . ./lib.sh

OUT=$1
mkdir -p "$OUT"

case $2 in
start)
  { gh --version | head -1; echo "api-version=$V"; date -u +%Y-%m-%dT%H:%M:%SZ; } > "$OUT/00-inputs.txt"
  rest "$OUT/b00-create-target.txt" POST issues \
    -f title="P-8 probe, bot target: title 0" -f body="Body 0, by the operator."
  N=$(sed '1,/^$/d' "$OUT/b00-create-target.txt" | jq -r .number)
  echo "target=$N" >> "$OUT/00-inputs.txt"
  sleep 2; observe "$OUT" b00-target "$N"

  MAIN=$(gh api "repos/$REPO/git/ref/heads/main" --jq .object.sha)
  rest "$OUT/b01-create-branch.txt" POST git/refs -f ref=refs/heads/probe/p-8 -f sha="$MAIN"
  sed "s/@TARGET@/$N/" probe-p8.yml > "$OUT/probe-p8.yml"
  rest "$OUT/b02-push-workflow.txt" PUT contents/.github/workflows/probe-p8.yml \
    -f message="Probe P-8: edit issues as github-actions[bot]" -f branch=probe/p-8 \
    -f content="$(base64 < "$OUT/probe-p8.yml" | tr -d '\n')"
  ;;
finish)
  N=$(sed -n 's/^target=//p' "$OUT/00-inputs.txt")
  X=$(gh api "repos/$REPO/issues?state=all&sort=created&direction=desc" \
    --jq '[.[] | select(.user.login == "github-actions[bot]")][0].number')
  echo "bots-own=$X" >> "$OUT/00-inputs.txt"
  RUN=$(gh run list -R "$REPO" --workflow probe-p8.yml --limit 1 --json databaseId --jq '.[0].databaseId')
  echo "run=$RUN" >> "$OUT/00-inputs.txt"
  gh run view "$RUN" -R "$REPO" --log > "$OUT/b03-run-log.txt" 2>&1

  observe "$OUT" b10-target-after-bot "$N"
  observe "$OUT" b10-own-after-bot "$X"

  rest "$OUT/b11-operator-renames-own.txt" PATCH "issues/$X" -f title="P-8 probe, bot's own: title 3, by the operator"
  sleep 2
  rest "$OUT/b12-operator-edits-own.txt" PATCH "issues/$X" -f body="Body 4, by the operator."
  sleep 2
  rest "$OUT/b13-operator-edits-target.txt" PATCH "issues/$N" -f body="Body 3, by the operator again."
  sleep 2
  observe "$OUT" b20-target-after-operator "$N"
  observe "$OUT" b20-own-after-operator "$X"
  ;;
esac
