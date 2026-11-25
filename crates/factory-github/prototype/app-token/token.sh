#!/usr/bin/env bash
# The first installation token of the scratch App: what it is given, and which
# repositories it covers. Reads only. Throwaway code.
#
# usage: token.sh <output directory>
set -u
cd "$(dirname "$0")" && . ./lib.sh
OUT=$1
mkdir -p "$OUT"

mint_token
jq '{expires_at, permissions, repository_selection, repositories}' "$OUT/00-token.json"

call 01-installation-repositories app "$V1" GET "installation/repositories?per_page=100"
jq -r '"total_count=\(.total_count) " + ([.repositories[].full_name] | join(","))' <<<"$BODY"

# The same token asked for a repository it was not restricted to, and for the App itself.
call 02-factory-repository app "$V1" GET "repos/$OWNER/software-factory"
call 03-app app "$V1" GET "app"
call 04-rate-limit app "$V1" GET "rate_limit"
jq -c '.resources | {core, graphql}' <<<"$BODY"

# Who the token is, by GraphQL.
graphql 05-viewer app 'query { viewer { __typename login databaseId id } }'
jq -c . <<<"$BODY"

# An unrestricted token of the same installation, to list what the installation covers.
resp=$(curl -sS -X POST "$API/app/installations/$INSTALLATION_ID/access_tokens" \
  -H "Authorization: Bearer $(app_jwt)" -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: $V1")
WIDE=$(jq -r '.token // empty' <<<"$resp")
jq '.token = "<removed>"' <<<"$resp" >"$OUT/06-token-unrestricted.json"
jq -c '{repository_selection, permissions}' "$OUT/06-token-unrestricted.json"
NARROW=$TOKEN
TOKEN=$WIDE
call 07-installation-repositories-unrestricted app "$V1" GET "installation/repositories?per_page=100"
jq -r '"total_count=\(.total_count) " + ([.repositories[].full_name] | join(","))' <<<"$BODY"
revoke_token
TOKEN=$NARROW
