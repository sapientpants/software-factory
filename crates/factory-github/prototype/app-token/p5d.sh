#!/usr/bin/env bash
# Probe P-5, the App part, fourth script, after a review of the record: the case
# nearest to publish on a repository that has a workflow. The App creates a branch
# through the REST API at a commit whose tree holds a workflow, the head of the
# operator's probe/p-5-app-base, and pushes one commit to it. Throwaway code.
#
# usage: p5d.sh <new output directory> <work directory of p5.sh>
set -u
cd "$(dirname "$0")" && . ./lib.sh
OUT=$(mkdir -p "$1" && cd "$1" && pwd)
WORK=$(cd "$2" && pwd)
mint_token
START=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "$START" >"$OUT/start.txt"

export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_TERMINAL_PROMPT=0
export GIT_AUTHOR_NAME="sf probe" GIT_AUTHOR_EMAIL="probe@sf-scratch.invalid"
export GIT_COMMITTER_NAME="sf probe" GIT_COMMITTER_EMAIL="probe@sf-scratch.invalid"
URL=https://github.com/$R.git
G="$WORK/repo"
ref_body() { jq -cn --arg r "refs/heads/$1" --arg s "$2" '{ref: $r, sha: $s}'; }

call 8-00-base app "$V1" GET "repos/$R/git/ref/heads/probe/p-5-app-base"
WFBASE=$(jq -r .object.sha <<<"$BODY")
WF=.github/workflows/p5app-base.yml
echo "base=$WFBASE"

# one <number> <case> <index operation>...: a branch at the base by POST /git/refs,
# then one commit pushed to it.
one() {
  local n=$1 c=$2 blob sha code
  shift 2
  call "8-$n-$c-create" app "$V1" POST "repos/$R/git/refs" "$(ref_body "probe/p-5-app-$c" "$WFBASE")"
  export GIT_INDEX_FILE="$WORK/index"
  rm -f "$GIT_INDEX_FILE"
  git -C "$G" read-tree "$WFBASE"
  case $1 in
    control)
      blob=$(printf 'control, on an existing branch that holds a workflow' | git -C "$G" hash-object -w --stdin)
      git -C "$G" update-index --add --cacheinfo "100644,$blob,p5app/control.txt"
      ;;
    modify)
      blob=$(printf 'name: p5app-base-changed\non:\n  push:\n    branches: ["probe/p-5-app-*"]\njobs:\n  echo:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo changed\n' | git -C "$G" hash-object -w --stdin)
      git -C "$G" update-index --add --cacheinfo "100644,$blob,$WF"
      ;;
    delete) git -C "$G" update-index --force-remove "$WF" ;;
  esac
  sha=$(git -C "$G" commit-tree "$(git -C "$G" write-tree)" -p "$WFBASE" -m "Probe P-5, App part: $c")
  unset GIT_INDEX_FILE
  {
    echo "push by app: $sha -> refs/heads/probe/p-5-app-$c"
    echo "the commit against its parent (git diff-tree -r --raw --no-renames):"
    git -C "$G" diff-tree -r --raw --no-renames "$sha^" "$sha"
    echo
  } >"$OUT/8-$n-$c.txt"
  PW=$TOKEN git -C "$G" -c credential.helper='!f() { echo username=x-access-token; echo "password=$PW"; }; f' \
    push --porcelain "$URL" "$sha:refs/heads/probe/p-5-app-$c" >>"$OUT/8-$n-$c.txt" 2>&1
  code=$?
  echo "exit=$code" >>"$OUT/8-$n-$c.txt"
  printf '%-40s app  exit=%s %s\n' "8-$n-$c" "$code" "$(grep -E '^(!|\*|=| )' "$OUT/8-$n-$c.txt" | grep -v '^ *$' | tail -1 | cut -c1-170)"
  sleep 1
  call "8-$n-$c-ref" app "$V1" GET "repos/$R/git/ref/heads/probe/p-5-app-$c"
}
one 01 upd-base-control control
one 02 upd-base-modify modify
one 03 upd-base-delete delete

sleep 75
call 8-10-runs app "$V1" GET "repos/$R/actions/runs?per_page=100&created=%3E%3D$START"
jq -r '.workflow_runs | sort_by(.created_at) | .[] | [.created_at, .event, .status, (.conclusion // "-"), .head_branch, .head_sha[0:7], .path, .actor.login, (.id | tostring)] | join(" | ")' <<<"$BODY" | tee "$OUT/8-10-runs-short.txt"
call 8-11-branches app "$V1" GET "repos/$R/git/matching-refs/heads/probe/p-5-app"
jq -r '.[] | "\(.object.sha[0:7]) \(.ref)"' <<<"$BODY" >"$OUT/8-11-branches-short.txt"
wc -l <"$OUT/8-11-branches-short.txt"
revoke_token | tee "$OUT/8-12-token-revoked.txt"
trap - EXIT
