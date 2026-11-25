#!/usr/bin/env bash
# Probe P-1 again: creating a ref that already exists, under the scratch App's
# installation token, first as the scratch repository stands and then with a
# ruleset on factory/**. Throwaway code.
#
# The operator's token creates and deletes each ruleset, since the App holds no
# Administration permission (GH-11). Every other request is the App's.
#
# usage: p1.sh <output directory>
set -u
cd "$(dirname "$0")" && . ./lib.sh
OUT=$1
mkdir -p "$OUT"
mint_token
op_token

ref_body() { jq -cn --arg r "$1" --arg s "$2" '{ref: $r, sha: $s}'; }
# create <name> <version> <ref> <sha>
create() { call "$1" app "$2" POST "repos/$R/git/refs" "$(ref_body "$3" "$4")"; }
# delete_ref <name> <who> <branch>
delete_ref() { call "$1" "$2" "$V1" DELETE "repos/$R/git/refs/heads/$3"; }

call 00a-main app "$V1" GET "repos/$R/git/ref/heads/main"
BASE=$(jq -r .object.sha <<<"$BODY")
call 00b-other app "$V1" GET "repos/$R/git/ref/heads/probe/p-16"
OTHER=$(jq -r .object.sha <<<"$BODY")
call 00c-base-commit app "$V1" GET "repos/$R/git/commits/$BASE"
TREE=$(jq -r .tree.sha <<<"$BODY")
call 00d-base-tree app "$V1" GET "repos/$R/git/trees/$TREE"
BLOB=$(jq -r '[.tree[] | select(.type == "blob")][0].sha' <<<"$BODY")
NONE=0123456789abcdef0123456789abcdef01234567
echo "base=$BASE other=$OTHER tree=$TREE blob=$BLOB"
call 00e-rulesets op "$V1" GET "repos/$R/rulesets"
call 00f-matching-factory app "$V1" GET "repos/$R/git/matching-refs/heads/factory"
echo "refs under factory before: $(jq -c '[.[].ref]' <<<"$BODY")"

# 1. The table of the first run, with the App's token and no ruleset.
for pair in "a $V1 7001" "b $V2 6001"; do
  set -- $pair
  t=$1
  v=$2
  n=$3
  ref=refs/heads/factory/issue-$n
  create "1$t-01-new-at-base" "$v" "$ref" "$BASE"
  create "1$t-02-again-at-base" "$v" "$ref" "$BASE"
  create "1$t-03-again-at-other" "$v" "$ref" "$OTHER"
  create "1$t-04-again-at-no-object" "$v" "$ref" "$NONE"
  create "1$t-05-again-short-sha" "$v" "$ref" abc123
  create "1$t-06-new-at-no-object" "$v" "refs/heads/factory/issue-$((n + 1))" "$NONE"
  create "1$t-07-new-short-sha" "$v" "refs/heads/factory/issue-$((n + 1))" abc123
  call "1$t-08-new-without-sha" app "$v" POST "repos/$R/git/refs" "{\"ref\":\"refs/heads/factory/issue-$((n + 1))\"}"
  create "1$t-09-new-at-tree" "$v" "refs/heads/factory/issue-$((n + 1))" "$TREE"
  create "1$t-10-new-at-blob" "$v" "refs/heads/factory/issue-$((n + 1))" "$BLOB"
  create "1$t-11-name-heads" "$v" "heads/factory/issue-$((n + 1))" "$BASE"
  create "1$t-12-name-bare" "$v" "factory/issue-$((n + 1))" "$BASE"
  create "1$t-13-name-dots" "$v" "refs/heads/factory/issue..$((n + 1))" "$BASE"
  create "1$t-14-name-space" "$v" "refs/heads/factory/issue $((n + 1))" "$BASE"
  create "1$t-15-below-existing" "$v" "$ref/sub" "$BASE"
  create "1$t-16-factory-while-child" "$v" "refs/heads/factory" "$BASE"
  call "1$t-17-no-credential" none "$v" POST "repos/$R/git/refs" "$(ref_body "refs/heads/factory/issue-$((n + 3))" "$BASE")"

  # The reads of 8.2 step 3.
  call "1$t-20-read-exact" app "$v" GET "repos/$R/git/ref/heads/factory/issue-$n"
  call "1$t-21-read-prefix" app "$v" GET "repos/$R/git/ref/heads/factory/issue-${n%?}"
  call "1$t-22-read-absent" app "$v" GET "repos/$R/git/ref/heads/factory/issue-$((n + 500))"
  call "1$t-23-read-factory" app "$v" GET "repos/$R/git/ref/heads/factory"
  call "1$t-24-read-sub" app "$v" GET "repos/$R/git/ref/heads/factory/issue-$n/sub"
  call "1$t-25-matching" app "$v" GET "repos/$R/git/matching-refs/heads/factory/issue-${n%?}"

  # A branch named factory in the way of a work branch.
  delete_ref "1$t-30-delete" app "factory/issue-$n"
  create "1$t-31-factory" "$v" "refs/heads/factory" "$BASE"
  create "1$t-32-child-while-factory" "$v" "$ref" "$BASE"
  call "1$t-33-read-child-while-factory" app "$v" GET "repos/$R/git/ref/heads/factory/issue-$n"
  delete_ref "1$t-34-delete-factory" app factory
done

# 2. Two creates at once, 10 pairs, each pair for one new ref on two connections.
#    Pairs 1 to 5 ask for the base twice, pairs 6 to 10 for the base and the other commit.
for i in 1 2 3 4 5 6 7 8 9 10; do
  ref=refs/heads/factory/issue-72$(printf '%02d' "$i")
  second=$BASE
  [ "$i" -gt 5 ] && second=$OTHER
  common=(-sS -X POST -H "Authorization: Bearer $TOKEN" -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: $V1")
  curl --parallel --parallel-immediate --parallel-max 2 \
    "${common[@]}" --data-binary "$(ref_body "$ref" "$BASE")" -o "$OUT/2-pair-$i-x.json" \
    -w "pair $i x %{http_code} %{time_total}\n" "$API/repos/$R/git/refs" \
    --next "${common[@]}" --data-binary "$(ref_body "$ref" "$second")" -o "$OUT/2-pair-$i-y.json" \
    -w "pair $i y %{http_code} %{time_total}\n" "$API/repos/$R/git/refs" | tee -a "$OUT/2-pairs.txt"
  call "2-pair-$i-read" app "$V1" GET "repos/$R/git/ref/heads/factory/issue-72$(printf '%02d' "$i")"
  printf 'pair %s: x=%s y=%s ref at %s (base %s, other %s)\n' "$i" \
    "$(jq -r '.message // "created"' "$OUT/2-pair-$i-x.json")" "$(jq -r '.message // "created"' "$OUT/2-pair-$i-y.json")" \
    "$(jq -r '.object.sha[0:7]' <<<"$BODY")" "${BASE:0:7}" "${OTHER:0:7}" | tee -a "$OUT/2-pairs.txt"
  sleep 1
done
for i in 1 2 3 4 5 6 7 8 9 10; do
  delete_ref "2-pair-$i-delete" app "factory/issue-72$(printf '%02d' "$i")"
done

# 3. With a ruleset on factory/**. For each variant: the App creates one ref while no
#    ruleset is active, the operator activates the ruleset, and the App creates that
#    ref again and a new one.
variant() {
  local k=$1 rules=$2 n=$3 id
  local ref=refs/heads/factory/issue-$n new=refs/heads/factory/issue-$((n + 1))
  create "3$k-01-before-ruleset" "$V1" "$ref" "$BASE"
  call "3$k-02-ruleset" op "$V1" POST "repos/$R/rulesets" "$(jq -cn --arg name "probe-p1-$k" --argjson rules "$rules" \
    '{name: $name, target: "branch", enforcement: "active", bypass_actors: [], conditions: {ref_name: {include: ["refs/heads/factory/**"], exclude: []}}, rules: $rules}')"
  id=$(jq -r '.id // empty' <<<"$BODY")
  if [ -z "$id" ]; then
    echo "no ruleset for variant $k"
    delete_ref "3$k-90-delete" app "factory/issue-$n"
    return
  fi
  sleep 2
  # Which rules GitHub applies to the two branch names, as each token reads them.
  call "3$k-03-rules-existing-app" app "$V1" GET "repos/$R/rules/branches/factory/issue-$n"
  echo "  rules on the existing branch: $(jq -c '[.[].type]' <<<"$BODY")"
  call "3$k-04-rules-new-app" app "$V1" GET "repos/$R/rules/branches/factory/issue-$((n + 1))"
  echo "  rules on the new branch name: $(jq -c '[.[].type]' <<<"$BODY")"
  call "3$k-05-rules-main-app" app "$V1" GET "repos/$R/rules/branches/main"
  echo "  rules on main: $(jq -c '[.[].type]' <<<"$BODY")"
  # What the App reads of the ruleset itself (DEP-4).
  call "3$k-06-rulesets-app" app "$V1" GET "repos/$R/rulesets"
  call "3$k-07-ruleset-app" app "$V1" GET "repos/$R/rulesets/$id"
  echo "  the App reads of the ruleset: $(jq -c '{bypass_actors, current_user_can_bypass, rules: [.rules[]?.type]}' <<<"$BODY")"
  call "3$k-08-ruleset-op" op "$V1" GET "repos/$R/rulesets/$id"
  echo "  the operator reads: $(jq -c '{bypass_actors, current_user_can_bypass, rules: [.rules[]?.type]}' <<<"$BODY")"

  create "3$k-10-again-at-base" "$V1" "$ref" "$BASE"
  create "3$k-11-again-at-other" "$V1" "$ref" "$OTHER"
  create "3$k-12-again-at-no-object" "$V1" "$ref" "$NONE"
  create "3$k-13-new-at-base" "$V1" "$new" "$BASE"
  create "3$k-14-new-at-base-again" "$V1" "$new" "$BASE"
  create "3$k-15-again-at-base-other-version" "$V2" "$ref" "$BASE"
  create "3$k-16-new-at-base-other-version" "$V2" "refs/heads/factory/issue-$((n + 2))" "$BASE"
  create "3$k-17-outside-pattern" "$V1" "refs/heads/probe/p-1-app-$k" "$BASE"
  call "3$k-20-read-existing" app "$V1" GET "repos/$R/git/ref/heads/factory/issue-$n"
  call "3$k-21-read-new" app "$V1" GET "repos/$R/git/ref/heads/factory/issue-$((n + 1))"
  # The App's delete under the ruleset, of the ref it created before.
  delete_ref "3$k-30-delete-under-ruleset" app "factory/issue-$n"

  call "3$k-80-delete-ruleset" op "$V1" DELETE "repos/$R/rulesets/$id"
  sleep 2
  for b in "factory/issue-$n" "factory/issue-$((n + 1))" "factory/issue-$((n + 2))" "probe/p-1-app-$k"; do
    call "3$k-90-read-$(tr '/' '_' <<<"$b")" app "$V1" GET "repos/$R/git/ref/heads/$b"
    if [ "$STATUS" = 200 ]; then delete_ref "3$k-91-delete-$(tr '/' '_' <<<"$b")" app "$b"; fi
  done
}

variant creation '[{"type":"creation"}]' 7301
variant update '[{"type":"update"}]' 7311
variant deletion '[{"type":"deletion"},{"type":"non_fast_forward"}]' 7321

# A sweep, in case a create that was expected to fail went through.
i=0
for prefix in heads/factory heads/probe/p-1-app; do
  call "98-sweep-list-$(tr '/' '_' <<<"$prefix")" app "$V1" GET "repos/$R/git/matching-refs/$prefix"
  for ref in $(jq -r '.[].ref' <<<"$BODY"); do
    i=$((i + 1))
    echo "left over: $ref"
    delete_ref "98-sweep-delete-$i" app "${ref#refs/heads/}"
  done
done

# What is left.
call 99a-rulesets op "$V1" GET "repos/$R/rulesets"
echo "rulesets after: $BODY"
call 99b-matching-factory app "$V1" GET "repos/$R/git/matching-refs/heads/factory"
echo "refs under factory after: $(jq -c '[.[].ref]' <<<"$BODY")"
call 99c-matching-probe app "$V1" GET "repos/$R/git/matching-refs/heads/probe/p-1"
echo "refs under probe/p-1 after: $(jq -c '[.[].ref]' <<<"$BODY")"
