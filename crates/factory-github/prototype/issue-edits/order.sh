#!/bin/bash
# Probe P-8, second part: can an edit be ordered against a `labeled` event?
# Rule 6 of SPEC 5.3 compares the two. Sends a label and an edit back to back,
# in both orders and at once, and reads what GitHub reports for each.
# Usage: [V=<api-version>] order.sh <output-dir>
set -u
cd "$(dirname "$0")" && . ./lib.sh

OUT=$1
mkdir -p "$OUT"
{ gh --version | head -1; echo "api-version=$V"; date -u +%Y-%m-%dT%H:%M:%SZ; } > "$OUT/00-inputs.txt"

rest "$OUT/o00-create.txt" POST issues -f title="P-8 probe, order: title 0" -f body="Body 0."
N=$(sed '1,/^$/d' "$OUT/o00-create.txt" | jq -r .number)
echo "issue=$N" >> "$OUT/00-inputs.txt"

label()   { rest "$OUT/$1.label.txt" POST "issues/$N/labels" -f "labels[]=$LABEL" > /dev/null; }
unlabel() { gh api --method DELETE "repos/$REPO/issues/$N/labels/$LABEL" > /dev/null 2>&1; sleep 2; }
body()    { rest "$OUT/$1.body.txt"  PATCH "issues/$N" -f body="Body of $1."   > /dev/null; }
title()   { rest "$OUT/$1.title.txt" PATCH "issues/$N" -f title="P-8 probe, order: title of $1" > /dev/null; }

# report <trial>: read, then one line with the times GitHub gives for the label and the edits
report() {
  sleep 2
  observe "$OUT" "$1" "$N" > /dev/null
  jq -r --arg t "$1" '.data.repository.issue as $i
    | ($i.timelineItems.nodes | map(select(.__typename != "UnlabeledEvent"))) as $ev
    | [$t,
       "labeled=" + ([$ev[] | select(.__typename == "LabeledEvent")] | last | .createdAt),
       "lastEditedAt=" + ($i.lastEditedAt // "null"),
       "renamed=" + (([$ev[] | select(.__typename == "RenamedTitleEvent")] | last | .createdAt) // "none"),
       "timeline-ends=" + ([$ev[-2:][] | .__typename] | join(","))] | join("\t")' \
    "$OUT/$1.read-graphql.json"
}

for i in 1 2 3 4 5 6; do
  unlabel; t=o1$i-label-then-body;  label $t; body $t;  report $t
  unlabel; t=o2$i-body-then-label;  body $t;  label $t; report $t
  unlabel; t=o3$i-label-then-title; label $t; title $t; report $t
  unlabel; t=o4$i-title-then-label; title $t; label $t; report $t
done

# both at once: the order on GitHub's side is not known to the sender
for i in 1 2 3 4; do
  unlabel; t=o5$i-label-and-body-at-once;  label $t & body $t & wait;  report $t
  unlabel; t=o6$i-label-and-title-at-once; label $t & title $t & wait; report $t
done

# one PATCH that edits the title and body and sets the label set (the operator
# could do this; the factory never replaces a label set, GH-1)
for i in 1 2; do
  unlabel; t=o7$i-one-call
  rest "$OUT/$t.patch.txt" PATCH "issues/$N" -f title="P-8 probe, order: title of $t" \
    -f body="Body of $t." -f "labels[]=$LABEL" > /dev/null
  report $t
done

# `gh issue edit` with a body and a label in one command
unlabel; t=o81-gh-issue-edit
gh issue edit "$N" -R "$REPO" --body "Body of $t." --add-label "$LABEL" > "$OUT/$t.gh.txt" 2>&1
report $t

echo "issue $N"
