#!/usr/bin/env bash
# Probe P-5, the App part, third script. p5b.sh showed that the App may not put a
# ref on its own commit that adds a workflow, though the commit is in the repository,
# while p5.sh showed that it may put one on such a commit of the operator's that is
# the head of a branch. This tries a commit of the operator's that is the head of no
# branch, and then lists the workflow runs and the workflows. Throwaway code.
#
# usage: p5c.sh <output directory of p5.sh> <work directory of p5.sh>
set -u
cd "$(dirname "$0")" && . ./lib.sh
OUT=$(cd "$1" && pwd)
WORK=$(cd "$2" && pwd)
mv "$OUT/00-token.json" "$OUT/00-token-second.json"
mint_token
op_token

export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_TERMINAL_PROMPT=0
export GIT_AUTHOR_NAME="sf probe" GIT_AUTHOR_EMAIL="probe@sf-scratch.invalid"
export GIT_COMMITTER_NAME="sf probe" GIT_COMMITTER_EMAIL="probe@sf-scratch.invalid"
URL=https://github.com/$R.git
G="$WORK/repo"
BASE=$(git -C "$G" rev-parse FETCH_HEAD)
ref_body() { jq -cn --arg r "refs/heads/$1" --arg s "$2" '{ref: $r, sha: $s}'; }

# 6. Two commits of the operator's: the first adds a workflow, the second removes it.
export GIT_INDEX_FILE="$WORK/index"
rm -f "$GIT_INDEX_FILE"
git -C "$G" read-tree "$BASE"
blob=$(printf 'name: p5app-op-two\non:\n  push:\n    branches: ["probe/p-5-app-*"]\njobs:\n  echo:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo p5app-op-two\n' | git -C "$G" hash-object -w --stdin)
git -C "$G" update-index --add --cacheinfo "100644,$blob,.github/workflows/p5app-op-two.yml"
O1=$(git -C "$G" commit-tree "$(git -C "$G" write-tree)" -p "$BASE" -m "Probe P-5, App part: the operator's two commits, 1 of 2")
git -C "$G" update-index --force-remove .github/workflows/p5app-op-two.yml
blob=$(printf 'control' | git -C "$G" hash-object -w --stdin)
git -C "$G" update-index --add --cacheinfo "100644,$blob,p5app/control.txt"
O2=$(git -C "$G" commit-tree "$(git -C "$G" write-tree)" -p "$O1" -m "Probe P-5, App part: the operator's two commits, 2 of 2")
unset GIT_INDEX_FILE
{
  echo "push by op: $O2 -> refs/heads/probe/p-5-app-op-two"
  git -C "$G" log --raw --no-renames --format='commit %H' "$BASE..$O2"
  echo
} >"$OUT/6-01-operator-two.txt"
PW=$OP_TOKEN git -C "$G" -c credential.helper='!f() { echo username=x-access-token; echo "password=$PW"; }; f' \
  push --porcelain "$URL" "$O2:refs/heads/probe/p-5-app-op-two" >>"$OUT/6-01-operator-two.txt" 2>&1
echo "exit=$?" >>"$OUT/6-01-operator-two.txt"
tail -3 "$OUT/6-01-operator-two.txt"
sleep 2
call 6-02-ref-at-operator-first app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-ref-at-op-first "$O1")"
# The same request with the operator's token, which holds the workflow scope.
call 6-03-ref-at-operator-first-op op "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-ref-at-op-first-op "$O1")"
# Now that a branch of the operator's has the commit as its head, the App again.
call 6-04-ref-at-operator-first-again app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-ref-at-op-first "$O1")"

# 7. The runs since the first push, and the workflows GitHub has registered, read
#    with the App's token (Actions: read).
sleep 45
START=$(cat "$OUT/start.txt")
call 7-01-runs app "$V1" GET "repos/$R/actions/runs?per_page=100&created=%3E%3D$START"
jq -r '.workflow_runs | sort_by(.created_at) | .[] | [.created_at, .event, .status, (.conclusion // "-"), .head_branch, .head_sha[0:7], .path, .actor.login, .triggering_actor.login, (.id | tostring)] | join(" | ")' <<<"$BODY" | tee "$OUT/7-01-runs-short.txt"
call 7-02-workflows app "$V1" GET "repos/$R/actions/workflows?per_page=100"
jq -r '.workflows[] | "\(.state) \(.path) \(.name)"' <<<"$BODY" | tee "$OUT/7-02-workflows-short.txt"
call 92-branches app "$V1" GET "repos/$R/git/matching-refs/heads/probe/p-5-app"
jq -r '.[] | "\(.object.sha[0:7]) \(.ref)"' <<<"$BODY" >"$OUT/92-branches-short.txt"
cat "$OUT/92-branches-short.txt"
