# Shared by the scripts of probe P-8. Sourced, not run.
# Everything here talks to the scratch repository only.

OWNER=sapientpants
NAME=sf-scratch
REPO=$OWNER/$NAME
V=${V:-2026-03-10}                # REST API version (GH-17 pins 2026-03-10)
LABEL=probe:p-8                   # a label with no meaning to the factory

# keep: cut response headers down to status, date, API version and request id.
# The rest names the scopes of the token and its rate limit.
keep() {
  awk 'BEGIN { h = 1 }
       h && /^\r?$/ { h = 0; print ""; next }
       h { l = tolower($0)
           if (/^HTTP\// || l ~ /^date:/ || l ~ /^x-github-api-version-selected:/ || l ~ /^x-github-request-id:/) print
           next }
       { print }' | tr -d '\r'
}

# rest <out-file> <method> <path> [gh api field arguments]: one REST call, response kept
rest() {
  out=$1; method=$2; path=$3; shift 3
  gh api -i --method "$method" "repos/$REPO/$path" \
    -H "X-GitHub-Api-Version: $V" "$@" 2>&1 | keep > "$out"
  printf '%s\t%s %s\t%s\n' "$(basename "$out")" "$method" "$path" "$(head -1 "$out")"
}

# gql <out-file> <query> [gh api field arguments]: one GraphQL call, response kept
gql() {
  out=$1; query=$2; shift 2
  gh api -i graphql -f query="$query" "$@" 2>&1 | keep > "$out"
  printf '%s\tgraphql\t%s\n' "$(basename "$out")" "$(head -1 "$out")"
}

READ_QUERY='
query($owner: String!, $name: String!, $n: Int!) {
  repository(owner: $owner, name: $name) {
    issue(number: $n) {
      number title body
      createdAt publishedAt updatedAt lastEditedAt
      includesCreatedEdit createdViaEmail
      author { __typename login }
      editor { __typename login }
      userContentEdits(first: 100) {
        totalCount
        nodes {
          createdAt editedAt deletedAt
          editor { __typename login }
          deletedBy { __typename login }
          diff
        }
      }
      timelineItems(first: 100, itemTypes: [RENAMED_TITLE_EVENT, LABELED_EVENT, UNLABELED_EVENT]) {
        totalCount
        nodes {
          __typename
          ... on RenamedTitleEvent { createdAt actor { __typename login } previousTitle currentTitle }
          ... on LabeledEvent { createdAt actor { __typename login } label { name } }
          ... on UnlabeledEvent { createdAt actor { __typename login } label { name } }
        }
      }
    }
  }
}'

# observe <dir> <step> <issue-number>: what the three read paths report now
observe() {
  gh api graphql -f query="$READ_QUERY" -f owner="$OWNER" -f name="$NAME" -F n="$3" \
    > "$1/$2.read-graphql.json" 2>&1
  gh api "repos/$REPO/issues/$3" -H "X-GitHub-Api-Version: $V" \
    --jq '{number, title, body, user: {login: .user.login, type: .user.type}, created_at, updated_at, state, labels: [.labels[].name]}' \
    > "$1/$2.read-rest-issue.json" 2>&1
  gh api "repos/$REPO/issues/$3/events" -H "X-GitHub-Api-Version: $V" \
    --jq '[.[] | {id, event, created_at, actor: {login: .actor.login, type: .actor.type}, rename, label: .label.name}]' \
    > "$1/$2.read-rest-events.json" 2>&1
  jq -r --arg step "$2" '.data.repository.issue
    | [$step, "lastEditedAt=" + (.lastEditedAt // "null"),
       "editor=" + (.editor.login // "null"),
       "edits=" + (.userContentEdits.totalCount | tostring),
       "renames=" + ([.timelineItems.nodes[] | select(.__typename == "RenamedTitleEvent")] | length | tostring),
       "updatedAt=" + .updatedAt] | join("\t")' "$1/$2.read-graphql.json"
}
