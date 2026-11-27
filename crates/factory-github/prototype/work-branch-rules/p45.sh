#!/usr/bin/env bash
# Probe P-45, for #98: what a rule on factory/** does to the writes that the factory
# makes to a work branch, and what the scratch App reads of the rule beforehand.
# One variant for each rule: the App makes two work branches while no rule is active,
# the operator's token activates the rule, and the App then sends each write once.
# Throwaway code.
#
# The operator's token creates and deletes each ruleset and each classic branch
# protection rule, since the App holds no Administration permission (GH-11), and makes
# the reads whose name ends in -op. Every other request and every push is the App's.
#
# usage: p45.sh <output directory> <work directory> [variant...]
set -u
cd "$(dirname "$0")" && . ./lib.sh
OUT=$(mkdir -p "$1" && cd "$1" && pwd)
WORK=$(mkdir -p "$2" && cd "$2" && pwd)
shift 2
ONLY=" $* "
mint_token
op_token
date -u +%Y-%m-%dT%H:%M:%SZ >>"$OUT/start.txt"

# git reads no configuration but the work repository's own: no credential helper of
# the Mac, no identity of the operator, no signing key.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_TERMINAL_PROMPT=0
export GIT_AUTHOR_NAME="sf probe" GIT_AUTHOR_EMAIL="probe@sf-scratch.invalid"
export GIT_COMMITTER_NAME="sf probe" GIT_COMMITTER_EMAIL="probe@sf-scratch.invalid"
URL=https://github.com/$R.git
TAB=$(printf '\t')
G="$WORK/repo"
rm -rf "$G"
git init -q "$G"
git -C "$G" fetch -q "$URL" refs/heads/main
BASE=$(git -C "$G" rev-parse FETCH_HEAD)
BEFORE=$(git -C "$G" rev-parse "$BASE^")
echo "base=$BASE before=$BEFORE"

# commit <message> <path> <content> <parent>...: prints a commit whose tree is the
# first parent's with one file added or changed.
commit() {
  local msg=$1 path=$2 content=$3 blob tree p parents=()
  shift 3
  export GIT_INDEX_FILE="$WORK/index"
  rm -f "$GIT_INDEX_FILE"
  git -C "$G" read-tree "$1"
  blob=$(printf '%s\n' "$content" | git -C "$G" hash-object -w --stdin)
  git -C "$G" update-index --add --cacheinfo "100644,$blob,$path"
  tree=$(git -C "$G" write-tree)
  unset GIT_INDEX_FILE
  for p in "$@"; do parents+=(-p "$p"); done
  git -C "$G" commit-tree "$tree" "${parents[@]}" -m "$msg"
}

# push <name> <sha> <branch> [git push option...]: one git push over HTTPS, as the App.
# Kept in $OUT/<name>.txt: the commit with its parents, git's output and its exit code.
push() {
  local name=$1 sha=$2 branch=$3 code
  shift 3
  {
    echo "push by app: $sha -> refs/heads/$branch $*"
    echo "the commit and its parents: $(git -C "$G" rev-list --parents -n 1 "$sha")"
    echo
  } >"$OUT/$name.txt"
  PW=$TOKEN git -C "$G" -c credential.helper='!f() { echo username=x-access-token; echo "password=$PW"; }; f' \
    push --porcelain "$@" "$URL" "$sha:refs/heads/$branch" >>"$OUT/$name.txt" 2>&1
  code=$?
  echo "exit=$code" >>"$OUT/$name.txt"
  printf '%-40s %-4s exit=%s %s\n' "$name" app "$code" \
    "$(grep -E "^(remote: |[!*=+ -]${TAB})" "$OUT/$name.txt" | sed -e 's/^remote: *//' | grep -v '^ *$' | tr '\n' '|' | cut -c1-330)"
  sleep 1
}

ref_body() { jq -cn --arg r "refs/heads/$1" --arg s "$2" '{ref: $r, sha: $s}'; }
create() { call "$1" app "$V1" POST "repos/$R/git/refs" "$(ref_body "$2" "$3")"; }
delete_ref() { call "$1" "$2" "$V1" DELETE "repos/$R/git/refs/heads/$3"; }
# head_of <name> <branch>: sets HEAD_SHA to the branch's head, or to nothing.
head_of() {
  call "$1" app "$V1" GET "repos/$R/git/ref/heads/$2"
  HEAD_SHA=$(jq -r '.object.sha // empty' <<<"$BODY" 2>/dev/null)
}

REPO_ID=
repo_id() {
  graphql "00c-repository-id" op 'query($o:String!,$n:String!){repository(owner:$o,name:$n){id}}' "{\"o\":\"$OWNER\",\"n\":\"$REPO\"}"
  REPO_ID=$(jq -r .data.repository.id <<<"$BODY")
}

RULES_Q='query($o:String!,$n:String!,$ref:String!){repository(owner:$o,name:$n){
  branchProtectionRules(first:20){nodes{pattern allowsDeletions allowsForcePushes blocksCreations lockBranch requiresApprovingReviews requiredApprovingReviewCount requiresCommitSignatures requiresLinearHistory requiresStatusChecks restrictsPushes isAdminEnforced}}
  ref(qualifiedName:$ref){name branchProtectionRule{pattern} refUpdateRule{pattern allowsDeletions allowsForcePushes blocksCreations requiredApprovingReviewCount requiresLinearHistory requiresSignatures requiredStatusCheckContexts viewerCanPush}}}}'

# variant <key> <number> <kind> <mode> <definition>
#   kind        none | ruleset | classic
#   mode        full: every write. short: the reads and the create alone.
#   definition  ruleset: the body of POST /rulesets without its name.
#               classic: the input of createBranchProtectionRule without the repository.
variant() {
  local k=$1 n=$2 kind=$3 mode=$4 def=$5 id=
  case "$ONLY" in "  " | *" $k "*) ;; *) return ;; esac
  local e=factory/issue-$n f=factory/issue-$((n + 1)) new=factory/issue-$((n + 2))
  local c1 c2 d r1 m
  echo "---- $k ($kind, $mode): $def"
  c1=$(commit "Probe P-45 $k: a first candidate" "probe/p-45/$k.txt" "one $k" "$BASE")
  c2=$(commit "Probe P-45 $k: a second candidate" "probe/p-45/$k.txt" "two $k" "$c1")
  d=$(commit "Probe P-45 $k: the default branch moved" "probe/p-45/$k-default.txt" "moved $k" "$BASE")
  r1=$(commit "Probe P-45 $k: a candidate after a reset" "probe/p-45/$k-reset.txt" "reset $k" "$BASE")

  # Before the rule: one work branch at the base, as the claim leaves it, and one that
  # holds a pushed candidate.
  create "$k-01-e-before" "$e" "$BASE"
  if [ "$mode" = full ]; then push "$k-02-f-before" "$c1" "$f"; fi

  case $kind in
    ruleset)
      call "$k-03-rule" op "$V1" POST "repos/$R/rulesets" "$(jq -cn --arg name "probe-p45-$k" --argjson d "$def" '{name: $name, target: "branch", bypass_actors: []} + $d')"
      id=$(jq -r '.id // empty' <<<"$BODY" 2>/dev/null)
      ;;
    classic)
      graphql "$k-03-rule" op 'mutation($i:CreateBranchProtectionRuleInput!){createBranchProtectionRule(input:$i){branchProtectionRule{id pattern}}}' \
        "$(jq -cn --arg r "$REPO_ID" --argjson d "$def" '{i: ({repositoryId: $r} + $d)}')"
      id=$(jq -r '.data.createBranchProtectionRule.branchProtectionRule.id // empty' <<<"$BODY" 2>/dev/null)
      [ -z "$id" ] && echo "  errors: $(jq -c '[.errors[]?.message]' <<<"$BODY" | cut -c1-300)"
      ;;
    none) id=none ;;
  esac
  if [ -z "$id" ]; then
    echo "  no rule for variant $k: $(jq -c '{message, errors}' <<<"$BODY" 2>/dev/null | cut -c1-300)"
  else
    sleep 3
    # What the App reads beforehand.
    call "$k-10-rules-e" app "$V1" GET "repos/$R/rules/branches/$e"
    echo "  rules on the existing branch: $(jq -c '[.[] | {type, ruleset_source_type, ruleset_id}]' <<<"$BODY" | cut -c1-300)"
    call "$k-11-rules-new" app "$V1" GET "repos/$R/rules/branches/$new?per_page=100"
    echo "  rules on the new branch name: $(jq -c '[.[] | {type, parameters}]' <<<"$BODY" | cut -c1-600)"
    call "$k-12-rules-main" app "$V1" GET "repos/$R/rules/branches/main"
    echo "  rules on main: $(jq -c '[.[].type]' <<<"$BODY")"
    case $kind in
      ruleset)
        call "$k-13-ruleset-app" app "$V1" GET "repos/$R/rulesets/$id"
        echo "  the App reads of the ruleset: $(jq -c '{enforcement, bypass_actors, current_user_can_bypass, rules: [.rules[]?.type]}' <<<"$BODY")"
        call "$k-14-rulesets-app" app "$V1" GET "repos/$R/rulesets"
        echo "  the App's list of rulesets: $(jq -c '[.[] | {name, enforcement}]' <<<"$BODY")"
        ;;
      classic)
        call "$k-13-branch-app" app "$V1" GET "repos/$R/branches/$e"
        echo "  the App reads of the branch: $(jq -c '{protected, protection}' <<<"$BODY" | cut -c1-300)"
        call "$k-14-protection-app" app "$V1" GET "repos/$R/branches/$e/protection"
        graphql "$k-15-graphql-app" app "$RULES_Q" "{\"o\":\"$OWNER\",\"n\":\"$REPO\",\"ref\":\"refs/heads/$e\"}"
        echo "  the App reads over GraphQL: $(jq -c '{rules: .data.repository.branchProtectionRules.nodes, ref: .data.repository.ref, errors: [.errors[]?.message]}' <<<"$BODY" | cut -c1-900)"
        graphql "$k-16-graphql-op" op "$RULES_Q" "{\"o\":\"$OWNER\",\"n\":\"$REPO\",\"ref\":\"refs/heads/$e\"}"
        echo "  the operator reads over GraphQL: $(jq -c '{rules: .data.repository.branchProtectionRules.nodes, errors: [.errors[]?.message]}' <<<"$BODY" | cut -c1-600)"
        ;;
    esac

    # The writes, each as the App.
    create "$k-20-create-new" "$new" "$BASE"                       # the claim, 8.2 step 3
    if [ "$mode" = full ]; then
      push "$k-21-push-first" "$c1" "$e"                           # publish, the first push to a ref at the base
      push "$k-22-push-next" "$c2" "$f"                            # publish, a later round
      head_of "$k-23-f-head" "$f"
      m=$(commit "Probe P-45 $k: a base update" "probe/p-45/$k-default.txt" "moved $k" "${HEAD_SHA:-$c1}" "$d")
      push "$k-24-push-merge" "$m" "$f"                            # the merge commit of a base update, 12.4
      head_of "$k-25-f-head" "$f"
      push "$k-26-push-lease" "$r1" "$f" "--force-with-lease=refs/heads/$f:${HEAD_SHA:-$c1}"   # a reset, 7.6
      call "$k-27-patch-force" app "$V1" PATCH "repos/$R/git/refs/heads/$e" "$(jq -cn --arg s "$BEFORE" '{sha: $s, force: true}')"
      delete_ref "$k-28-delete" app "$e"                           # the branch of a merged run, 18.2
      call "$k-30-e-after" app "$V1" GET "repos/$R/git/ref/heads/$e"
      echo "  e afterwards: $STATUS $(jq -r '.object.sha // empty' <<<"$BODY" | cut -c1-7) (base ${BASE:0:7}, c1 ${c1:0:7}, before ${BEFORE:0:7})"
      call "$k-31-f-after" app "$V1" GET "repos/$R/git/ref/heads/$f"
      echo "  f afterwards: $STATUS $(jq -r '.object.sha // empty' <<<"$BODY" | cut -c1-7) (c1 ${c1:0:7}, c2 ${c2:0:7}, merge ${m:0:7}, reset ${r1:0:7})"
    fi
    call "$k-32-new-after" app "$V1" GET "repos/$R/git/ref/heads/$new"
    echo "  the new ref afterwards: $STATUS"
  fi

  case $kind:$id in
    ruleset:?*) call "$k-80-delete-rule" op "$V1" DELETE "repos/$R/rulesets/$id" ;;
    classic:?*) graphql "$k-80-delete-rule" op 'mutation($id:ID!){deleteBranchProtectionRule(input:{branchProtectionRuleId:$id}){clientMutationId}}' "{\"id\":\"$id\"}" ;;
  esac
  sleep 3
  for b in "$e" "$f" "$new"; do
    call "$k-90-read-${b#factory/}" app "$V1" GET "repos/$R/git/ref/heads/$b"
    if [ "$STATUS" = 200 ]; then delete_ref "$k-91-delete-${b#factory/}" app "$b"; fi
  done
}

call 00a-base-commit app "$V1" GET "repos/$R/commits/$BASE"
echo "the base commit: verified=$(jq -c .commit.verification.verified <<<"$BODY") reason=$(jq -r .commit.verification.reason <<<"$BODY") committer=$(jq -r .commit.committer.name <<<"$BODY")"
call 00b-before-commit app "$V1" GET "repos/$R/commits/$BEFORE"
echo "the commit before it: verified=$(jq -c .commit.verification.verified <<<"$BODY") reason=$(jq -r .commit.verification.reason <<<"$BODY")"
repo_id
call 00d-rulesets op "$V1" GET "repos/$R/rulesets"
echo "rulesets before: $BODY"
call 00e-matching-factory app "$V1" GET "repos/$R/git/matching-refs/heads/factory"
echo "refs under factory before: $(jq -c '[.[].ref]' <<<"$BODY")"

rs() { # <enforcement> <include> <rules>
  jq -cn --arg e "$1" --arg i "$2" --argjson r "$3" '{enforcement: $e, conditions: {ref_name: {include: [$i], exclude: []}}, rules: $r}'
}
FACTORY='refs/heads/factory/**'
PR='{"type":"pull_request","parameters":{"required_approving_review_count":0,"dismiss_stale_reviews_on_push":false,"require_code_owner_review":false,"require_last_push_approval":false,"required_review_thread_resolution":false}}'
CHECKS='{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"probe/p-45"}],"strict_required_status_checks_policy":false}}'
CHECKS_CREATE='{"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"probe/p-45"}],"strict_required_status_checks_policy":false,"do_not_enforce_on_create":true}}'

variant control 7500 none full '{}'
variant creation 7510 ruleset full "$(rs active "$FACTORY" '[{"type":"creation"}]')"
variant update 7520 ruleset full "$(rs active "$FACTORY" '[{"type":"update"}]')"
variant deletion 7530 ruleset full "$(rs active "$FACTORY" '[{"type":"deletion"}]')"
variant non-fast-forward 7540 ruleset full "$(rs active "$FACTORY" '[{"type":"non_fast_forward"}]')"
variant linear-history 7550 ruleset full "$(rs active "$FACTORY" '[{"type":"required_linear_history"}]')"
variant signatures 7560 ruleset full "$(rs active "$FACTORY" '[{"type":"required_signatures"}]')"
variant pull-request 7570 ruleset full "$(rs active "$FACTORY" "[$PR]")"
variant status-checks 7580 ruleset full "$(rs active "$FACTORY" "[$CHECKS]")"
variant status-checks-create 7590 ruleset full "$(rs active "$FACTORY" "[$CHECKS_CREATE]")"
variant deployments 7600 ruleset full "$(rs active "$FACTORY" '[{"type":"required_deployments","parameters":{"required_deployment_environments":["probe-p-45"]}}]')"
variant message-pattern 7610 ruleset full "$(rs active "$FACTORY" '[{"type":"commit_message_pattern","parameters":{"operator":"starts_with","pattern":"zzz"}}]')"
variant branch-name-pattern 7620 ruleset short "$(rs active "$FACTORY" '[{"type":"branch_name_pattern","parameters":{"operator":"starts_with","pattern":"zzz"}}]')"

# How the rule is attached: another pattern, a bypass actor, a ruleset that is not active.
variant all-branches 7630 ruleset short "$(rs active '~ALL' '[{"type":"creation"}]')"
variant bypass-admin 7640 ruleset short "$(rs active "$FACTORY" '[{"type":"creation"}]' | jq -c '. + {bypass_actors: [{actor_id: 5, actor_type: "RepositoryRole", bypass_mode: "always"}]}')"
variant evaluate 7650 ruleset short "$(rs evaluate "$FACTORY" '[{"type":"creation"}]')"
variant disabled 7660 ruleset short "$(rs disabled "$FACTORY" '[{"type":"creation"}]')"

# A classic branch protection rule in place of a ruleset.
variant classic-default 7700 classic full '{"pattern":"factory/*"}'
variant classic-reviews 7710 classic full '{"pattern":"factory/*","requiresApprovingReviews":true,"requiredApprovingReviewCount":1}'
variant classic-lock 7720 classic full '{"pattern":"factory/*","lockBranch":true}'
variant classic-creations 7730 classic short '{"pattern":"factory/*","restrictsPushes":true,"blocksCreations":true}'

# A sweep, in case a rule or a ref was left.
call 97-sweep-rulesets op "$V1" GET "repos/$R/rulesets"
for id in $(jq -r '.[] | select(.name | startswith("probe-p45-")) | .id' <<<"$BODY"); do
  echo "left over: ruleset $id"
  call "97-sweep-ruleset-$id" op "$V1" DELETE "repos/$R/rulesets/$id"
done
graphql 97-sweep-classic op 'query($o:String!,$n:String!){repository(owner:$o,name:$n){branchProtectionRules(first:20){nodes{id pattern}}}}' "{\"o\":\"$OWNER\",\"n\":\"$REPO\"}"
i=0
for id in $(jq -r '.data.repository.branchProtectionRules.nodes[] | select(.pattern == "factory/*") | .id' <<<"$BODY"); do
  i=$((i + 1))
  echo "left over: classic rule $id"
  graphql "97-sweep-classic-$i" op 'mutation($id:ID!){deleteBranchProtectionRule(input:{branchProtectionRuleId:$id}){clientMutationId}}' "{\"id\":\"$id\"}"
done
sleep 2
call 98-sweep-list app "$V1" GET "repos/$R/git/matching-refs/heads/factory"
i=0
for ref in $(jq -r '.[].ref' <<<"$BODY"); do
  i=$((i + 1))
  echo "left over: $ref"
  delete_ref "98-sweep-delete-$i" app "${ref#refs/heads/}"
done

# What is left.
call 99a-rulesets op "$V1" GET "repos/$R/rulesets"
echo "rulesets after: $BODY"
graphql 99b-classic op 'query($o:String!,$n:String!){repository(owner:$o,name:$n){branchProtectionRules(first:20){nodes{id pattern}}}}' "{\"o\":\"$OWNER\",\"n\":\"$REPO\"}"
echo "classic rules after: $(jq -c .data.repository.branchProtectionRules.nodes <<<"$BODY")"
call 99c-matching-factory app "$V1" GET "repos/$R/git/matching-refs/heads/factory"
echo "refs under factory after: $(jq -c '[.[].ref]' <<<"$BODY")"
date -u +%Y-%m-%dT%H:%M:%SZ >>"$OUT/end.txt"
