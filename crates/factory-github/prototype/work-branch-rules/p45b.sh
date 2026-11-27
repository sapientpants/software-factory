#!/usr/bin/env bash
# Probe P-45, second script, for #98. Two things that p45.sh left unsaid:
#   1. Under a rule for a linear history the create of a ref at the head of main was
#      refused. Is a create accepted at a commit with no merge commit behind it?
#   2. The App's GraphQL read of branchProtectionRules was refused and took the whole
#      response with it. What does the App read of a classic rule through the ref alone,
#      for a ref that exists and for a name that no ref has?
# Throwaway code. The operator's token creates and deletes the two rules.
#
# usage: p45b.sh <output directory> <work directory>
set -u
cd "$(dirname "$0")" && . ./lib.sh
OUT=$(mkdir -p "$1" && cd "$1" && pwd)
WORK=$(mkdir -p "$2" && cd "$2" && pwd)
mint_token
op_token
date -u +%Y-%m-%dT%H:%M:%SZ >"$OUT/start.txt"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_TERMINAL_PROMPT=0
G="$WORK/repo"
rm -rf "$G"
git init -q "$G"
git -C "$G" fetch -q "https://github.com/$R.git" refs/heads/main
BASE=$(git -C "$G" rev-parse FETCH_HEAD)
# The newest commit of main's first-parent line that has no merge commit behind it.
FIRST_MERGE=$(git -C "$G" rev-list --merges --reverse "$BASE" | head -1)
LINEAR=$(git -C "$G" rev-parse "$FIRST_MERGE^1")
echo "base=$BASE merges-behind-base=$(git -C "$G" rev-list --count --merges "$BASE")"
echo "linear=$LINEAR merges-behind-linear=$(git -C "$G" rev-list --count --merges "$LINEAR")"

ref_body() { jq -cn --arg r "refs/heads/$1" --arg s "$2" '{ref: $r, sha: $s}'; }
create() { call "$1" app "$V1" POST "repos/$R/git/refs" "$(ref_body "$2" "$3")"; }
delete_ref() { call "$1" "$2" "$V1" DELETE "repos/$R/git/refs/heads/$3"; }
VARS() { jq -cn --arg o "$OWNER" --arg n "$REPO" --arg ref "refs/heads/$1" '{o: $o, n: $n, ref: $ref}'; }

# ---- 1. A linear history.
call 1-01-rule op "$V1" POST "repos/$R/rulesets" '{"name":"probe-p45-linear-history-2","target":"branch","enforcement":"active","bypass_actors":[],"conditions":{"ref_name":{"include":["refs/heads/factory/**"],"exclude":[]}},"rules":[{"type":"required_linear_history"}]}'
RS=$(jq -r '.id // empty' <<<"$BODY")
sleep 3
create 1-02-create-at-base factory/issue-7800 "$BASE"
create 1-03-create-at-linear factory/issue-7801 "$LINEAR"
call 1-04-read-at-base app "$V1" GET "repos/$R/git/ref/heads/factory/issue-7800"
call 1-05-read-at-linear app "$V1" GET "repos/$R/git/ref/heads/factory/issue-7801"
[ -n "$RS" ] && call 1-80-delete-rule op "$V1" DELETE "repos/$R/rulesets/$RS"
sleep 3

# ---- 2. What the App reads of a classic rule through a ref.
create 2-01-e-before factory/issue-7810 "$BASE"
graphql 2-02-repository-id op 'query($o:String!,$n:String!){repository(owner:$o,name:$n){id}}' "$(jq -cn --arg o "$OWNER" --arg n "$REPO" '{o: $o, n: $n}')"
REPO_ID=$(jq -r .data.repository.id <<<"$BODY")
graphql 2-03-rule op 'mutation($i:CreateBranchProtectionRuleInput!){createBranchProtectionRule(input:$i){branchProtectionRule{id pattern}}}' \
  "$(jq -cn --arg r "$REPO_ID" '{i: {repositoryId: $r, pattern: "factory/*", requiresApprovingReviews: true, requiredApprovingReviewCount: 1}}')"
CL=$(jq -r '.data.createBranchProtectionRule.branchProtectionRule.id // empty' <<<"$BODY")
sleep 3
Q_UPDATE='query($o:String!,$n:String!,$ref:String!){repository(owner:$o,name:$n){ref(qualifiedName:$ref){name refUpdateRule{pattern allowsDeletions allowsForcePushes blocksCreations requiredApprovingReviewCount requiresLinearHistory requiresSignatures requiredStatusCheckContexts viewerCanPush}}}}'
Q_RULE='query($o:String!,$n:String!,$ref:String!){repository(owner:$o,name:$n){ref(qualifiedName:$ref){name branchProtectionRule{pattern requiresApprovingReviews}}}}'
for who in app op; do
  graphql "2-10-update-rule-existing-$who" "$who" "$Q_UPDATE" "$(VARS factory/issue-7810)"
  echo "  $who, refUpdateRule of the existing ref: $(jq -c '{ref: .data.repository.ref, errors: [.errors[]?.message]}' <<<"$BODY")"
  graphql "2-11-update-rule-absent-$who" "$who" "$Q_UPDATE" "$(VARS factory/issue-7811)"
  echo "  $who, refUpdateRule of a name that no ref has: $(jq -c '{ref: .data.repository.ref, errors: [.errors[]?.message]}' <<<"$BODY")"
  graphql "2-12-protection-rule-existing-$who" "$who" "$Q_RULE" "$(VARS factory/issue-7810)"
  echo "  $who, branchProtectionRule of the existing ref: $(jq -c '{data: .data, errors: [.errors[]? | {path, message}]}' <<<"$BODY")"
done
call 2-20-branch-existing app "$V1" GET "repos/$R/branches/factory/issue-7810"
echo "  the App, the branch: $(jq -c '{protected, protection}' <<<"$BODY")"
call 2-21-branch-absent app "$V1" GET "repos/$R/branches/factory/issue-7811"
call 2-22-branches-protected app "$V1" GET "repos/$R/branches?protected=true&per_page=100"
echo "  the App, the list of protected branches: $(jq -c '[.[] | {name, protected}]' <<<"$BODY")"
[ -n "$CL" ] && graphql 2-80-delete-rule op 'mutation($id:ID!){deleteBranchProtectionRule(input:{branchProtectionRuleId:$id}){clientMutationId}}' "{\"id\":\"$CL\"}"
sleep 3

# ---- 3. The same read through the ref under a ruleset, for what refUpdateRule covers.
call 3-01-rule op "$V1" POST "repos/$R/rulesets" '{"name":"probe-p45-update-2","target":"branch","enforcement":"active","bypass_actors":[],"conditions":{"ref_name":{"include":["refs/heads/factory/**"],"exclude":[]}},"rules":[{"type":"update"},{"type":"non_fast_forward"}]}'
RS=$(jq -r '.id // empty' <<<"$BODY")
sleep 3
graphql 3-10-update-rule-existing-app app "$Q_UPDATE" "$(VARS factory/issue-7810)"
echo "  app, refUpdateRule of the existing ref under a ruleset: $(jq -c '{ref: .data.repository.ref, errors: [.errors[]?.message]}' <<<"$BODY")"
call 3-11-branch-existing app "$V1" GET "repos/$R/branches/factory/issue-7810"
echo "  the App, the branch under a ruleset: $(jq -c '{protected, protection}' <<<"$BODY")"
[ -n "$RS" ] && call 3-80-delete-rule op "$V1" DELETE "repos/$R/rulesets/$RS"
sleep 3

# ---- What is left.
for b in factory/issue-7800 factory/issue-7801 factory/issue-7810; do
  call "90-read-${b#factory/}" app "$V1" GET "repos/$R/git/ref/heads/$b"
  if [ "$STATUS" = 200 ]; then delete_ref "91-delete-${b#factory/}" app "$b"; fi
done
call 99a-rulesets op "$V1" GET "repos/$R/rulesets"
echo "rulesets after: $(jq -c . <<<"$BODY")"
graphql 99b-classic op 'query($o:String!,$n:String!){repository(owner:$o,name:$n){branchProtectionRules(first:20){nodes{id pattern}}}}' "$(jq -cn --arg o "$OWNER" --arg n "$REPO" '{o: $o, n: $n}')"
echo "classic rules after: $(jq -c .data.repository.branchProtectionRules.nodes <<<"$BODY")"
call 99c-matching-factory app "$V1" GET "repos/$R/git/matching-refs/heads/factory"
echo "refs under factory after: $(jq -c '[.[].ref]' <<<"$BODY")"
date -u +%Y-%m-%dT%H:%M:%SZ >"$OUT/end.txt"
