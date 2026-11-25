#!/usr/bin/env bash
# Probe P-5, the App part, second script. p5.sh showed that the App's push of two
# commits is accepted when the first adds a workflow and the second removes it, and
# that the App may put a ref on a commit of the operator's that adds a workflow.
# This asks whether the App may then put a ref on its own first commit. Throwaway code.
#
# usage: p5b.sh <output directory of p5.sh> <work directory of p5.sh>
set -u
cd "$(dirname "$0")" && . ./lib.sh
OUT=$(cd "$1" && pwd)
WORK=$(cd "$2" && pwd)
mv "$OUT/00-token.json" "$OUT/00-token-first.json"
mint_token
op_token

export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_TERMINAL_PROMPT=0
export GIT_AUTHOR_NAME="sf probe" GIT_AUTHOR_EMAIL="probe@sf-scratch.invalid"
export GIT_COMMITTER_NAME="sf probe" GIT_COMMITTER_EMAIL="probe@sf-scratch.invalid"
URL=https://github.com/$R.git
G="$WORK/repo"
BASE=$(git -C "$G" rev-parse FETCH_HEAD)

workflow() {
  printf 'name: %s\non:\n  push:\n    branches: ["probe/p-5-app-*"]\njobs:\n  echo:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo %s\n' "$1" "$1"
}
ref_body() { jq -cn --arg r "refs/heads/$1" --arg s "$2" '{ref: $r, sha: $s}'; }
push() {
  local name=$1 sha=$2 branch=$3 code
  {
    echo "push by app: $sha -> refs/heads/$branch"
    echo "the commits of the push, newest first, each against its parent (git log --raw --no-renames):"
    git -C "$G" log --raw --no-renames --format='commit %H' "$BASE..$sha"
    echo
  } >"$OUT/$name.txt"
  PW=$TOKEN git -C "$G" -c credential.helper='!f() { echo username=x-access-token; echo "password=$PW"; }; f' \
    push --porcelain "$URL" "$sha:refs/heads/$branch" >>"$OUT/$name.txt" 2>&1
  code=$?
  echo "exit=$code" >>"$OUT/$name.txt"
  printf '%-40s app  exit=%s %s\n' "$name" "$code" "$(grep -E '^(!|\*|=| )' "$OUT/$name.txt" | grep -v '^ *$' | tail -1 | cut -c1-170)"
  sleep 1
}
# two <path> <name>: a commit that adds a workflow and a control file, and its child that removes the workflow.
two() {
  local blob tree
  export GIT_INDEX_FILE="$WORK/index"
  rm -f "$GIT_INDEX_FILE"
  git -C "$G" read-tree "$BASE"
  blob=$(workflow "$2" | git -C "$G" hash-object -w --stdin)
  git -C "$G" update-index --add --cacheinfo "100644,$blob,$1"
  blob=$(printf 'control %s' "$2" | git -C "$G" hash-object -w --stdin)
  git -C "$G" update-index --add --cacheinfo "100644,$blob,p5app/control.txt"
  FIRST=$(git -C "$G" commit-tree "$(git -C "$G" write-tree)" -p "$BASE" -m "Probe P-5, App part: $2, 1 of 2")
  git -C "$G" update-index --force-remove "$1"
  SECOND=$(git -C "$G" commit-tree "$(git -C "$G" write-tree)" -p "$FIRST" -m "Probe P-5, App part: $2, 2 of 2")
  unset GIT_INDEX_FILE
}

# 5a. The first commit of the two that p5.sh pushed to probe/p-5-app-upd-add-remove.
call 5a-00-tip app "$V1" GET "repos/$R/git/ref/heads/probe/p-5-app-upd-add-remove"
C1=$(git -C "$G" rev-parse "$(jq -r .object.sha <<<"$BODY")^")
echo "the first commit: $C1"
call 5a-01-ref-at-own app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-ref-at-own "$C1")"
call 5a-02-move-branch app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-move-to-own "$BASE")"
call 5a-03-move-to-own app "$V1" PATCH "repos/$R/git/refs/heads/probe/p-5-app-move-to-own" "$(jq -cn --arg s "$C1" '{sha: $s}')"
push 5a-04-push-at-own "$C1" probe/p-5-app-push-at-own
call 5a-04-push-at-own-ref app "$V1" GET "repos/$R/git/ref/heads/probe/p-5-app-push-at-own"

# 5b. The same two commits as a new branch, made by the push.
two .github/workflows/p5app-new-gone.yml p5app-new-gone
push 5b-01-new-add-remove "$SECOND" probe/p-5-app-new-add-remove
call 5b-01-new-add-remove-ref app "$V1" GET "repos/$R/git/ref/heads/probe/p-5-app-new-add-remove"
call 5b-02-ref-at-first app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-new-first "$FIRST")"

# 5c. Two more, pushed to a branch that is then deleted, so that no ref reaches the
#     first commit when a ref is created at it.
two .github/workflows/p5app-unreachable.yml p5app-unreachable
push 5c-01-temporary "$SECOND" probe/p-5-app-temporary
call 5c-02-delete-temporary app "$V1" DELETE "repos/$R/git/refs/heads/probe/p-5-app-temporary"
call 5c-03-ref-at-unreachable app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-ref-at-unreachable "$FIRST")"

# 5d. A commit that no push has sent: the REST API asked for a ref at it.
two .github/workflows/p5app-never-pushed.yml p5app-never-pushed
call 5d-01-ref-at-unknown app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-ref-at-unknown "$FIRST")"

call 91-branches app "$V1" GET "repos/$R/git/matching-refs/heads/probe/p-5-app"
jq -r '.[] | "\(.object.sha[0:7]) \(.ref)"' <<<"$BODY" >"$OUT/91-branches-short.txt"
wc -l <"$OUT/91-branches-short.txt"
