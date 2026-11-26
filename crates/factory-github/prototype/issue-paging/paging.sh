#!/usr/bin/env bash
# Probe P-8, the paging part: what a read of an issue returns where its edit history,
# its events or its comments hold more than one page, how each is asked from the end,
# and what a cursor does after the issue changed. Throwaway code.
#
# usage: paging.sh <output directory>
#
# One issue on the scratch repository. The operator's token, as gh holds it, makes the
# writes but four, which the scratch App's installation token makes so that an account
# other than the operator stands on the oldest page of each list.
set -u
cd "$(dirname "$0")" && . ./lib.sh
OUT=$1
mkdir -p "$OUT"
mint_token
op_token

TEXT="Probe P-8 of the software factory specification, the paging part (issue 91 of the factory repository)."
EDITS=105 # the last numbered body, so 106 history entries with the text as created

# quiet <name> <app|op> <method> <path> <body>: a write of which only the status and the
# date are kept, one line in 1-writes.tsv. For the writes that are made in a loop.
quiet() {
  local name=$1 who=$2 method=$3 path=$4 body=$5 tok tmp code
  tmp=$(mktemp)
  case $who in app) tok=$TOKEN ;; *) tok=$OP_TOKEN ;; esac
  code=$(curl -sS -D "$tmp" -o /dev/null -w '%{http_code}' -X "$method" "$API/$path" \
    -H "Authorization: Bearer $tok" -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: $V1" --data-binary "$body")
  printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$who" "$method" "$code" \
    "$(grep -i '^date:' "$tmp" | tr -d '\r' | cut -d' ' -f2-)" >>"$OUT/1-writes.tsv"
  rm -f "$tmp"
  case $code in 2*) ;; *)
    echo "write $name failed with $code; stopping" >&2
    exit 1
    ;;
  esac
  sleep 1
}

issue_body() { jq -n --arg t "$TEXT" --arg i "$1" '$t + "\n\nBody " + $i + "."'; }

# ---------------------------------------------------------------------------------
# 1. The issue and its history.
call 1a-create op "$V1" POST "repos/$R/issues" \
  "{\"title\":\"P-8 probe: paging, title 0\",\"body\":$(issue_body 0)}"
N=$(jq -r .number <<<"$BODY")
echo "$N" >"$OUT/issue-number.txt"
echo "issue: $N"

# The App edits and renames first, so its two entries are the oldest but one.
quiet edit-body-1 app PATCH "repos/$R/issues/$N" "{\"body\":$(issue_body '1, by the scratch App')}"
quiet rename-1 app PATCH "repos/$R/issues/$N" '{"title":"P-8 probe: paging, title 1, by the scratch App"}'
quiet comment-1 app POST "repos/$R/issues/$N/comments" '{"body":"Comment 1, by the scratch App."}'
quiet label-add-1 op POST "repos/$R/issues/$N/labels" '{"labels":["probe:p-8"]}'
quiet label-remove-1 op DELETE "repos/$R/issues/$N/labels/probe:p-8" ''

i=2
while [ $i -le $EDITS ]; do
  quiet "edit-$i" op PATCH "repos/$R/issues/$N" \
    "{\"title\":\"P-8 probe: paging, title $i\",\"body\":$(issue_body $i)}"
  quiet "comment-$i" op POST "repos/$R/issues/$N/comments" "{\"body\":\"Comment $i.\"}"
  if [ $i -eq 50 ]; then
    quiet label-add-2 op POST "repos/$R/issues/$N/labels" '{"labels":["probe:p-8"]}'
  fi
  i=$((i + 1))
done
quiet label-remove-2 op DELETE "repos/$R/issues/$N/labels/probe:p-8" ''
quiet label-add-3 op POST "repos/$R/issues/$N/labels" '{"labels":["probe:p-8"]}'
# One rename after the last label, so the last labeled event is not the last event.
quiet rename-106 op PATCH "repos/$R/issues/$N" '{"title":"P-8 probe: paging, title 106"}'
echo "writes: $(wc -l <"$OUT/1-writes.tsv" | tr -d ' ')"

# ---------------------------------------------------------------------------------
# The queries. Each asks for one connection, with the arguments as variables.
WHO='__typename login ... on User { databaseId } ... on Bot { databaseId }'
PAGE='pageInfo { hasNextPage hasPreviousPage startCursor endCursor }'
HEAD="query(\$n: Int!, \$first: Int, \$after: String, \$last: Int, \$before: String"
ISSUE="rateLimit { cost remaining } repository(owner: \"$OWNER\", name: \"$REPO\") { issue(number: \$n) {
  title updatedAt lastEditedAt includesCreatedEdit author { $WHO } editor { $WHO }"
EDITS_C="userContentEdits(first: \$first, after: \$after, last: \$last, before: \$before) {
    totalCount $PAGE
    edges { cursor node { editedAt createdAt deletedAt editor { $WHO } diff } }
  }"
TL_C="timelineItems(first: \$first, after: \$after, last: \$last, before: \$before, itemTypes: \$types) {
    totalCount filteredCount pageCount updatedAt $PAGE
    edges { cursor node {
      __typename
      ... on RenamedTitleEvent { createdAt actor { $WHO } currentTitle }
      ... on LabeledEvent { createdAt actor { $WHO } label { name } }
      ... on UnlabeledEvent { createdAt actor { $WHO } label { name } }
    } }
  }"
COMMENTS_C="comments(first: \$first, after: \$after, last: \$last, before: \$before) {
    totalCount $PAGE
    edges { cursor node { databaseId createdAt author { $WHO } isMinimized body } }
  }"
Q_EDITS="$HEAD) { $ISSUE $EDITS_C } } }"
Q_TL="$HEAD, \$types: [IssueTimelineItemsItemType!]) { $ISSUE $TL_C } } }"
Q_COMMENTS="$HEAD) { $ISSUE $COMMENTS_C } } }"
THREE='["RENAMED_TITLE_EVENT","LABELED_EVENT","UNLABELED_EVENT"]'

# The claim's read as one query: the newest page of each list.
Q_ALL="query(\$n: Int!, \$types: [IssueTimelineItemsItemType!]) { $ISSUE body
  userContentEdits(first: 100) { totalCount $PAGE edges { cursor node { editedAt editor { $WHO } } } }
  timelineItems(last: 100, itemTypes: \$types) { totalCount filteredCount $PAGE
    edges { cursor node { __typename
      ... on RenamedTitleEvent { createdAt actor { $WHO } }
      ... on LabeledEvent { createdAt actor { $WHO } label { name } }
      ... on UnlabeledEvent { createdAt actor { $WHO } label { name } } } } }
  comments(last: 100) { totalCount $PAGE edges { cursor node { databaseId createdAt author { $WHO } isMinimized } } }
} } }"

# vars <first|null> <after|null> <last|null> <before|null> [types]
vars() {
  jq -cn --argjson n "$N" --argjson first "$1" --arg after "$2" --argjson last "$3" --arg before "$4" \
    --argjson types "${5:-null}" \
    '{n: $n, first: $first, last: $last, types: $types,
      after: (if $after == "null" then null else $after end),
      before: (if $before == "null" then null else $before end)}'
}
# cursor <name> <connection> <startCursor|endCursor>
cursor() { body_of "$1" | jq -r ".data.repository.issue.$2.pageInfo.$3"; }

# reads <who>: every read of section 2 to 6, with one token.
reads() {
  local w=$1 c

  # 2. The edit history. It comes newest first, so the first page is the newest.
  graphql "2a-edits-first100-$w" "$w" "$Q_EDITS" "$(vars 100 null null null)"
  c=$(cursor "2a-edits-first100-$w" userContentEdits endCursor)
  graphql "2b-edits-first100-after-$w" "$w" "$Q_EDITS" "$(vars 100 "$c" null null)"
  graphql "2c-edits-last100-$w" "$w" "$Q_EDITS" "$(vars null null 100 null)"
  c=$(cursor "2c-edits-last100-$w" userContentEdits startCursor)
  graphql "2d-edits-last100-before-$w" "$w" "$Q_EDITS" "$(vars null null 100 "$c")"
  graphql "2e-edits-first101-$w" "$w" "$Q_EDITS" "$(vars 101 null null null)"
  graphql "2f-edits-first50-p1-$w" "$w" "$Q_EDITS" "$(vars 50 null null null)"
  c=$(cursor "2f-edits-first50-p1-$w" userContentEdits endCursor)
  graphql "2f-edits-first50-p2-$w" "$w" "$Q_EDITS" "$(vars 50 "$c" null null)"
  c=$(cursor "2f-edits-first50-p2-$w" userContentEdits endCursor)
  graphql "2f-edits-first50-p3-$w" "$w" "$Q_EDITS" "$(vars 50 "$c" null null)"
  graphql "2g-edits-no-count-$w" "$w" "$Q_EDITS" "$(vars null null null null)"

  # 3. The events. They come oldest first, so the newest page is asked with `last`.
  graphql "3a-tl-first100-$w" "$w" "$Q_TL" "$(vars 100 null null null "$THREE")"
  c=$(cursor "3a-tl-first100-$w" timelineItems endCursor)
  graphql "3b-tl-first100-after-$w" "$w" "$Q_TL" "$(vars 100 "$c" null null "$THREE")"
  graphql "3c-tl-last100-$w" "$w" "$Q_TL" "$(vars null null 100 null "$THREE")"
  c=$(cursor "3c-tl-last100-$w" timelineItems startCursor)
  graphql "3d-tl-last100-before-$w" "$w" "$Q_TL" "$(vars null null 100 "$c" "$THREE")"
  graphql "3e-tl-last1-labeled-$w" "$w" "$Q_TL" "$(vars null null 1 null '["LABELED_EVENT"]')"
  graphql "3f-tl-last1-renamed-$w" "$w" "$Q_TL" "$(vars null null 1 null '["RENAMED_TITLE_EVENT"]')"
  graphql "3g-tl-last101-$w" "$w" "$Q_TL" "$(vars null null 101 null "$THREE")"
  graphql "3h-tl-last100-all-types-$w" "$w" "$Q_TL" "$(vars null null 100 null null)"

  # 4. The comments. Oldest first as well.
  graphql "4a-comments-last100-$w" "$w" "$Q_COMMENTS" "$(vars null null 100 null)"
  c=$(cursor "4a-comments-last100-$w" comments startCursor)
  graphql "4b-comments-last100-before-$w" "$w" "$Q_COMMENTS" "$(vars null null 100 "$c")"
  graphql "4c-comments-first100-$w" "$w" "$Q_COMMENTS" "$(vars 100 null null null)"

  # 5. REST, for the events alone: the edit history is not in REST (P-8).
  call "5a-rest-events-default-$w" "$w" "$V1" GET "repos/$R/issues/$N/events"
  call "5b-rest-events-100-p1-$w" "$w" "$V1" GET "repos/$R/issues/$N/events?per_page=100"
  call "5c-rest-events-100-p2-$w" "$w" "$V1" GET "repos/$R/issues/$N/events?per_page=100&page=2"
  call "5d-rest-events-100-p9-$w" "$w" "$V1" GET "repos/$R/issues/$N/events?per_page=100&page=9"

  # 6. The newest page of all three in one query.
  graphql "6a-all-newest-$w" "$w" "$Q_ALL" "$(jq -cn --argjson n "$N" --argjson types "$THREE" '{n: $n, types: $types}')"
}

reads op
reads app

# ---------------------------------------------------------------------------------
# 7. A cursor after the issue changed. The cursors are those of the operator's reads
# above. Then one more edit with a rename, one more comment and one more label event,
# and the next page of each list is asked with the old cursor.
E_OLD=$(cursor 2a-edits-first100-op userContentEdits endCursor)
T_OLD=$(cursor 3c-tl-last100-op timelineItems startCursor)
C_OLD=$(cursor 4a-comments-last100-op comments startCursor)
call 7a-edit-107 op "$V1" PATCH "repos/$R/issues/$N" \
  "{\"title\":\"P-8 probe: paging, title 107\",\"body\":$(issue_body 107)}"
call 7b-comment-106 op "$V1" POST "repos/$R/issues/$N/comments" '{"body":"Comment 106."}'
call 7c-label-remove-3 op "$V1" DELETE "repos/$R/issues/$N/labels/probe:p-8"
graphql 7d-edits-after-old-cursor op "$Q_EDITS" "$(vars 100 "$E_OLD" null null)"
graphql 7e-tl-before-old-cursor op "$Q_TL" "$(vars null null 100 "$T_OLD" "$THREE")"
graphql 7f-comments-before-old-cursor op "$Q_COMMENTS" "$(vars null null 100 "$C_OLD")"
graphql 7g-edits-first100-fresh op "$Q_EDITS" "$(vars 100 null null null)"
graphql 7h-tl-last100-fresh op "$Q_TL" "$(vars null null 100 null "$THREE")"
graphql 7i-comments-last100-fresh op "$Q_COMMENTS" "$(vars null null 100 null)"
graphql 7j-all-newest-fresh op "$Q_ALL" "$(jq -cn --argjson n "$N" --argjson types "$THREE" '{n: $n, types: $types}')"

# A deleted comment: does the old cursor of the comments still page?
D=$(body_of 4a-comments-last100-op | jq -r '.data.repository.issue.comments.edges[0].node.databaseId')
call 7k-delete-comment op "$V1" DELETE "repos/$R/issues/comments/$D"
graphql 7l-comments-before-cursor-of-deleted op "$Q_COMMENTS" "$(vars null null 100 "$C_OLD")"

call 9a-close op "$V1" PATCH "repos/$R/issues/$N" '{"state":"closed"}'
