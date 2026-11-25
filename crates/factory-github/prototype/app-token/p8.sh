#!/usr/bin/env bash
# Probe P-8 again: what the scratch App's installation token reads of an issue's
# editors and edit times, set against what the operator's token reads at the same
# moment. Then a few writes by the App, to see how GitHub names it. Throwaway code.
#
# usage: p8.sh <output directory>
set -u
cd "$(dirname "$0")" && . ./lib.sh
OUT=$1
mkdir -p "$OUT"
mint_token
op_token

# The query of the first run of P-8, with the numeric id of each account added: SEC-2
# recognises the factory's App by it.
WHO='__typename login ... on User { databaseId } ... on Bot { databaseId }'
QUERY="query(\$n: Int!) { repository(owner: \"$OWNER\", name: \"$REPO\") { issue(number: \$n) {
  title body createdAt updatedAt lastEditedAt includesCreatedEdit
  author { $WHO }
  editor { $WHO }
  userContentEdits(first: 100) {
    totalCount
    nodes { createdAt editedAt deletedAt editor { $WHO } deletedBy { $WHO } diff }
  }
  timelineItems(first: 100, itemTypes: [RENAMED_TITLE_EVENT, LABELED_EVENT, UNLABELED_EVENT]) {
    totalCount
    nodes {
      __typename
      ... on RenamedTitleEvent { createdAt actor { $WHO } previousTitle currentTitle }
      ... on LabeledEvent { createdAt actor { $WHO } label { name } }
      ... on UnlabeledEvent { createdAt actor { $WHO } label { name } }
    }
  }
} } }"

# read_issue <prefix> <n>: the three reads of the first run, once with each token.
read_issue() {
  local p=$1 n=$2 who
  for who in app op; do
    graphql "$p-graphql-$who" "$who" "$QUERY" "{\"n\": $n}"
    call "$p-rest-issue-$who" "$who" "$V1" GET "repos/$R/issues/$n"
    call "$p-rest-events-$who" "$who" "$V1" GET "repos/$R/issues/$n/events?per_page=100"
  done
  for kind in graphql rest-issue rest-events; do
    if diff -q <(body_of "$p-$kind-app" | jq -S .) <(body_of "$p-$kind-op" | jq -S .) >/dev/null; then
      echo "  #$n $kind: the two tokens read the same"
    else
      echo "  #$n $kind: THE TWO TOKENS READ DIFFERENTLY"
    fi
  done
}

# 1. The issues of the first run, as they stand.
for n in 5 6 7 8 9 10; do
  read_issue "1-issue-$n" "$n"
done

# 2. The permission endpoint that SEC-1 checks a labeller through.
call 2a-permission-operator app "$V1" GET "repos/$R/collaborators/$OWNER/permission"
jq -c '{permission, role_name, user: .user.login}' <<<"$BODY"
call 2b-permission-stranger app "$V1" GET "repos/$R/collaborators/octocat/permission"
jq -c '{permission, role_name, user: .user.login}' <<<"$BODY"
call 2c-permission-actions-bot app "$V1" GET "repos/$R/collaborators/github-actions%5Bbot%5D/permission"
jq -c '{permission, role_name, user: .user.login, message}' <<<"$BODY"
call 2d-permission-operator-op op "$V1" GET "repos/$R/collaborators/$OWNER/permission"
jq -c '{permission, role_name, user: .user.login}' <<<"$BODY"

# 3. An issue that the App files and edits itself: how each API names the App.
call 3a-create app "$V1" POST "repos/$R/issues" \
  '{"title":"P-8 probe: an issue filed by the scratch App","body":"Probe P-8 of the software factory specification, run again under the scratch App'"'"'s installation token (issue 82 of the factory repository).\n\nBody 0."}'
N=$(jq -r .number <<<"$BODY")
echo "$N" >"$OUT/issue-number.txt"
echo "issue: $N"
call 3b-edit-body app "$V1" PATCH "repos/$R/issues/$N" \
  '{"body":"Probe P-8 of the software factory specification, run again under the scratch App'"'"'s installation token (issue 82 of the factory repository).\n\nBody 1."}'
call 3c-edit-title app "$V1" PATCH "repos/$R/issues/$N" '{"title":"P-8 probe: an issue filed and renamed by the scratch App"}'
call 3d-label app "$V1" POST "repos/$R/issues/$N/labels" '{"labels":["probe:p-8"]}'
# The operator edits and renames it after the App.
call 3e-edit-body-operator op "$V1" PATCH "repos/$R/issues/$N" \
  '{"body":"Probe P-8 of the software factory specification, run again under the scratch App'"'"'s installation token (issue 82 of the factory repository).\n\nBody 2, by the operator."}'
call 3f-edit-title-operator op "$V1" PATCH "repos/$R/issues/$N" '{"title":"P-8 probe: an issue filed by the scratch App, renamed by it and by the operator"}'
read_issue 3g-own "$N"
call 3h-close app "$V1" PATCH "repos/$R/issues/$N" '{"state":"closed"}'

# 4. The App's bot account by its login, and the App by its slug.
call 4a-bot-account app "$V1" GET "users/sf-scratch-app%5Bbot%5D"
jq -c '{login, id, node_id, type}' <<<"$BODY"
call 4b-app-by-slug app "$V1" GET "apps/sf-scratch-app"
jq -c '{id, slug, owner: .owner.login}' <<<"$BODY"
