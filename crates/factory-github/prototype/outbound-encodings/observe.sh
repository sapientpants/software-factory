#!/bin/bash
# Probe P-15, third part: read back everything the probe wrote. Run after the workflow,
# after its commits reach main, after the pull requests are merged, and after each later round.
#   items.json           one GraphQL read (read.graphql) of every issue and pull request of the probe
#   timelines-rest.json  GET /issues/{n}/timeline of each, by number
#   commits.json         the commits of the branch probe/p-15-commits that main did not have
#   commits-html.json    the probe's commits on main and on the two branches of the later rounds,
#                        with their messages as GitHub renders them and the first of those branches that has them
#   notifications.json   the operator's notifications for the scratch repository
# Usage: observe.sh <results-dir> <observation-name>
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/lib.sh"

OUT=$1; OBS=$1/$2
mkdir -p "$OBS"
date -u +%Y-%m-%dT%H:%M:%SZ > "$OBS/00-read-at.txt"

NUMBERS=$(gh api --paginate "repos/$REPO/issues?state=all&per_page=100" -H "X-GitHub-Api-Version: $V" \
  --jq '.[] | select(.title | startswith("P-15 probe:")) | .number' | sort -n)
echo "$(echo "$NUMBERS" | wc -l | tr -d ' ') issues and pull requests"

T=$(mktemp -d)
for n in $NUMBERS; do
  gh api graphql -f query="$(cat "$HERE/read.graphql")" -f owner="$OWNER" -f name="$NAME" -F n="$n" \
    --jq '.data.repository.issueOrPullRequest' > "$T/item-$n.json" 2> "$T/item-$n.err" || cat "$T/item-$n.err"
  gh api --paginate "repos/$REPO/issues/$n/timeline?per_page=100" -H "X-GitHub-Api-Version: $V" \
    --jq '.[] | {event, created_at: (.created_at // .submitted_at), actor: (.actor.login // .user.login),
                 commit_id, state_reason, source: (if .source then {type: .source.type, number: .source.issue.number} else null end)}' \
    | jq -s --arg n "$n" '{($n): .}' > "$T/timeline-$n.json"
done
jq -s . "$T"/item-*.json > "$OBS/items.json"
jq -s 'add' "$T"/timeline-*.json > "$OBS/timelines-rest.json"
rm -rf "$T"

MAIN=$(sed -n 's/^main=//p' "$OUT/00-inputs.txt")
gh api "repos/$REPO/compare/$MAIN...probe/p-15-commits" -H "X-GitHub-Api-Version: $V" \
  --jq '[.commits[] | {sha, message: .commit.message, author: .author.login, committer: .committer.login,
                       date: .commit.committer.date}]' > "$OBS/commits.json" 2>&1

for b in main probe/p-15-variants probe/p-15-operator; do
  gh api graphql -f owner="$OWNER" -f name="$NAME" -f ref="refs/heads/$b" -f query='
    query($owner: String!, $name: String!, $ref: String!) {
      repository(owner: $owner, name: $name) { ref(qualifiedName: $ref) { target { ... on Commit {
        history(first: 100) { nodes { oid messageHeadline messageHeadlineHTML messageBodyHTML } } } } } } }' \
    --jq '.data.repository.ref.target.history.nodes[]? | select(.messageHeadline | startswith("P-15 probe:"))' \
    | jq -c --arg b "$b" '. + {branch: $b}'
done | jq -s 'group_by(.oid) | map(.[0])' > "$OBS/commits-html.json"   # a commit on two branches: under the first

gh api --paginate "repos/$REPO/notifications?all=true&per_page=50" -H "X-GitHub-Api-Version: $V" \
  --jq '.[] | {id, reason, unread, updated_at, subject: {type: .subject.type, title: .subject.title, url: .subject.url}}' \
  | jq -s . > "$OBS/notifications.json"

echo "items=$(jq length "$OBS/items.json") commits=$(jq length "$OBS/commits.json") notifications=$(jq length "$OBS/notifications.json")"
