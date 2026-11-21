#!/usr/bin/env bash
# Probe P-25: what happens when a label that does not exist is added to an issue.
# Runs against the scratch repository only. Throwaway code.
#
# usage: probe.sh <output directory>
set -u

R=sapientpants/sf-scratch
OUT=$1
mkdir -p "$OUT"

# call <name> <gh api arguments...>: one request, response with headers kept in $OUT/<name>.txt
call() {
  local name=$1
  shift
  gh api -i "$@" >"$OUT/$name.txt" 2>&1
  printf '%-28s exit=%s  %s\n' "$name" "$?" "$(head -1 "$OUT/$name.txt" | tr -d '\r')"
}

body() { printf '{"labels":["%s"]}' "$1"; }

# The issue every label call is made on.
call 00-create-issue --method POST "repos/$R/issues" \
  -H "X-GitHub-Api-Version: 2026-03-10" \
  -f title="P-25 probe: adding a label that does not exist" \
  -f body="Probe P-25 of the software factory's specification. Labels named p25:* are created and deleted by the probe."
N=$(sed -n '/^{/,$p' "$OUT/00-create-issue.txt" | jq -r .number)
echo "issue: $N"
echo "$N" >"$OUT/issue-number.txt"

# The gh command line, which has no API version of its own to set.
call 01-cli-before "repos/$R/labels/p25:cli"
gh issue edit "$N" -R "$R" --add-label "p25:cli" >"$OUT/02-cli-add-label.txt" 2>&1
echo "02-cli-add-label             exit=$?  $(head -1 "$OUT/02-cli-add-label.txt")"
call 03-cli-after "repos/$R/labels/p25:cli"

# GraphQL adds labels by node id, so a missing label has to be looked up first.
gh api graphql -f query='query { repository(owner: "sapientpants", name: "sf-scratch") { label(name: "p25:graphql") { id name } } }' \
  >"$OUT/04-graphql-lookup.txt" 2>&1
echo "04-graphql-lookup            exit=$?  $(head -1 "$OUT/04-graphql-lookup.txt")"

for pair in "a 2026-03-10" "b 2022-11-28"; do
  set -- $pair
  t=$1
  v=$2
  H="X-GitHub-Api-Version: $v"

  # POST /issues/{n}/labels: the add-label endpoint of GH-1.
  call "1$t-post-before" -H "$H" "repos/$R/labels/p25:post-$t"
  body "p25:post-$t" | call "2$t-post" --method POST -H "$H" "repos/$R/issues/$N/labels" --input -
  call "3$t-post-after" -H "$H" "repos/$R/labels/p25:post-$t"

  # PUT /issues/{n}/labels: replaces the whole set, which GH-1 forbids.
  call "4$t-put-before" -H "$H" "repos/$R/labels/p25:put-$t"
  body "p25:put-$t" | call "5$t-put" --method PUT -H "$H" "repos/$R/issues/$N/labels" --input -
  call "6$t-put-after" -H "$H" "repos/$R/labels/p25:put-$t"

  # PATCH /issues/{n} with a labels key.
  call "7$t-patch-before" -H "$H" "repos/$R/labels/p25:patch-$t"
  body "p25:patch-$t" | call "8$t-patch" --method PATCH -H "$H" "repos/$R/issues/$N" --input -
  call "9$t-patch-after" -H "$H" "repos/$R/labels/p25:patch-$t"

  # POST /issues with a labels key: a new issue born with the label.
  call "A$t-create-before" -H "$H" "repos/$R/labels/p25:create-$t"
  printf '{"title":"P-25 probe: created with a label that does not exist (%s)","labels":["p25:create-%s"]}' "$v" "$t" |
    call "B$t-create" --method POST -H "$H" "repos/$R/issues" --input -
  call "C$t-create-after" -H "$H" "repos/$R/labels/p25:create-$t"
done

# What the issue and its timeline say afterwards.
call Z1-issue-labels -H "X-GitHub-Api-Version: 2026-03-10" "repos/$R/issues/$N/labels"
call Z2-timeline -H "X-GitHub-Api-Version: 2026-03-10" "repos/$R/issues/$N/timeline?per_page=100"
call Z3-repo-labels -H "X-GitHub-Api-Version: 2026-03-10" "repos/$R/labels?per_page=100"
