#!/usr/bin/env bash
# Probe P-45, third script, for #98, after a review of the record. Two things that
# the first two scripts did not try:
#   1. The remedy that GH-22 names: a ruleset for every branch that excludes
#      refs/heads/factory/**. Does the rules read then list nothing for a work branch,
#      and do the App's create, force-update and delete go through?
#   2. The operator's own read of the rules of a branch name, which onboarding by hand
#      uses. Does it list what the App's read lists, also where the operator's role
#      may bypass the ruleset?
# Throwaway code. The operator's token creates and deletes the rulesets and makes the
# reads whose name ends in -op.
#
# usage: p45c.sh <output directory>
set -u
cd "$(dirname "$0")" && . ./lib.sh
OUT=$(mkdir -p "$1" && cd "$1" && pwd)
mint_token
op_token
date -u +%Y-%m-%dT%H:%M:%SZ >"$OUT/start.txt"

ref_body() { jq -cn --arg r "refs/heads/$1" --arg s "$2" '{ref: $r, sha: $s}'; }
create() { call "$1" app "$V1" POST "repos/$R/git/refs" "$(ref_body "$2" "$3")"; }
delete_ref() { call "$1" "$2" "$V1" DELETE "repos/$R/git/refs/heads/$3"; }
types() { jq -c '[.[] | {type, ruleset_id}]' <<<"$BODY"; }
ruleset() { # <name> <key> <include> <exclude as JSON> <bypass as JSON>
  call "$1" op "$V1" POST "repos/$R/rulesets" "$(jq -cn --arg name "probe-p45-$2" --arg i "$3" --argjson x "$4" --argjson b "$5" \
    '{name: $name, target: "branch", enforcement: "active", bypass_actors: $b, conditions: {ref_name: {include: [$i], exclude: $x}}, rules: [{type: "creation"}, {type: "non_fast_forward"}, {type: "deletion"}]}')"
  RS=$(jq -r '.id // empty' <<<"$BODY")
  sleep 3
}

call 00a-main app "$V1" GET "repos/$R/git/ref/heads/main"
BASE=$(jq -r .object.sha <<<"$BODY")
call 00b-base-commit app "$V1" GET "repos/$R/git/commits/$BASE"
BEFORE=$(jq -r '.parents[0].sha' <<<"$BODY")
echo "base=$BASE before=$BEFORE"

# ---- 1. A ruleset for every branch that excludes the work branches.
ruleset 1-01-rule exclude '~ALL' '["refs/heads/factory/**"]' '[]'
call 1-10-rules-factory-app app "$V1" GET "repos/$R/rules/branches/factory/issue-7900?per_page=100"
echo "  the App, a name under factory/: $(types)"
call 1-11-rules-other-app app "$V1" GET "repos/$R/rules/branches/probe/p-45-c-outside?per_page=100"
echo "  the App, a name outside it: $(types)"
call 1-12-rules-main-app app "$V1" GET "repos/$R/rules/branches/main?per_page=100"
echo "  the App, main: $(types)"
create 1-20-create-factory factory/issue-7900 "$BASE"
create 1-21-create-outside probe/p-45-c-outside "$BASE"
call 1-22-patch-force-factory app "$V1" PATCH "repos/$R/git/refs/heads/factory/issue-7900" "$(jq -cn --arg s "$BEFORE" '{sha: $s, force: true}')"
delete_ref 1-23-delete-factory app factory/issue-7900
call 1-24-read-factory app "$V1" GET "repos/$R/git/ref/heads/factory/issue-7900"
call 1-25-read-outside app "$V1" GET "repos/$R/git/ref/heads/probe/p-45-c-outside"
[ -n "$RS" ] && call 1-80-delete-rule op "$V1" DELETE "repos/$R/rulesets/$RS"
sleep 3

# ---- 2. The operator's read beside the App's.
for pair in 'a []' 'b [{"actor_id":5,"actor_type":"RepositoryRole","bypass_mode":"always"}]'; do
  k=${pair%% *}
  ruleset "2$k-01-rule" "operator-read-$k" 'refs/heads/factory/**' '[]' "${pair#* }"
  call "2$k-10-rules-app" app "$V1" GET "repos/$R/rules/branches/factory/issue-7910?per_page=100"
  A=$(jq -cS . <<<"$BODY")
  echo "  the App: $(types)"
  call "2$k-11-rules-op" op "$V1" GET "repos/$R/rules/branches/factory/issue-7910?per_page=100"
  O=$(jq -cS . <<<"$BODY")
  echo "  the operator: $(types)"
  [ "$A" = "$O" ] && echo "  the two bodies are the same" || echo "  the two bodies differ"
  call "2$k-12-ruleset-op" op "$V1" GET "repos/$R/rulesets/$RS"
  echo "  the operator reads of the ruleset: $(jq -c '{bypass_actors, current_user_can_bypass}' <<<"$BODY")"
  [ -n "$RS" ] && call "2$k-80-delete-rule" op "$V1" DELETE "repos/$R/rulesets/$RS"
  sleep 3
done

# ---- What is left.
for b in factory/issue-7900 probe/p-45-c-outside; do
  call "90-read-$(tr '/' '_' <<<"$b")" app "$V1" GET "repos/$R/git/ref/heads/$b"
  if [ "$STATUS" = 200 ]; then delete_ref "91-delete-$(tr '/' '_' <<<"$b")" app "$b"; fi
done
call 99a-rulesets op "$V1" GET "repos/$R/rulesets"
echo "rulesets after: $(jq -c . <<<"$BODY")"
call 99c-matching-factory app "$V1" GET "repos/$R/git/matching-refs/heads/factory"
echo "refs under factory after: $(jq -c '[.[].ref]' <<<"$BODY")"
date -u +%Y-%m-%dT%H:%M:%SZ >"$OUT/end.txt"
