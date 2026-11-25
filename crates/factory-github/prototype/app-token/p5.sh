#!/usr/bin/env bash
# Probe P-5, the App part: which pushes GitHub refuses the scratch App, which holds
# no Workflows permission (GH-11). Each case is one commit on a branch of its own,
# probe/p-5-app-<case>, pushed with the App's installation token over HTTPS or
# written through the REST API. Throwaway code.
#
# The operator's token pushes two branches that the cases build on, and nothing else.
#
# usage: p5.sh <output directory> <work directory>
set -u
cd "$(dirname "$0")" && . ./lib.sh
OUT=$(mkdir -p "$1" && cd "$1" && pwd)
WORK=$(mkdir -p "$2" && cd "$2" && pwd)
mint_token
op_token
START=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "$START" >"$OUT/start.txt"

# git reads no configuration but the work repository's own: no credential helper of
# the Mac, no identity of the operator.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_TERMINAL_PROMPT=0
export GIT_AUTHOR_NAME="sf probe" GIT_AUTHOR_EMAIL="probe@sf-scratch.invalid"
export GIT_COMMITTER_NAME="sf probe" GIT_COMMITTER_EMAIL="probe@sf-scratch.invalid"
URL=https://github.com/$R.git
G="$WORK/repo"
rm -rf "$G"
git init -q "$G"
git -C "$G" config core.ignorecase false
git -C "$G" fetch -q "$URL" refs/heads/main
BASE=$(git -C "$G" rev-parse FETCH_HEAD)
FORM=$(git -C "$G" rev-parse "$BASE:.github/ISSUE_TEMPLATE/factory-task.yml")
echo "base=$BASE"

workflow() {
  printf 'name: %s\non:\n  push:\n    branches: ["probe/p-5-app-*"]\njobs:\n  echo:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo %s\n' "$1" "$1"
}

# commit <parent> <message> <operation>...: prints the new commit. Operations:
#   file <path> <content>   a regular file, mode 100644
#   exec <path> <content>   a regular file, mode 100755
#   link <path> <target>    a symbolic link
#   blob <path> <sha>       a regular file of an existing blob
#   rm <path>
commit() {
  local parent=$1 msg=$2 blob tree
  shift 2
  export GIT_INDEX_FILE="$WORK/index"
  rm -f "$GIT_INDEX_FILE"
  git -C "$G" read-tree "$parent"
  while [ $# -gt 0 ]; do
    case $1 in
      file | exec)
        blob=$(printf '%s' "$3" | git -C "$G" hash-object -w --stdin)
        git -C "$G" update-index --add --cacheinfo "$([ "$1" = exec ] && echo 100755 || echo 100644),$blob,$2"
        shift 3
        ;;
      link)
        blob=$(printf '%s' "$3" | git -C "$G" hash-object -w --stdin)
        git -C "$G" update-index --add --cacheinfo "120000,$blob,$2"
        shift 3
        ;;
      blob)
        git -C "$G" update-index --add --cacheinfo "100644,$3,$2"
        shift 3
        ;;
      rm)
        git -C "$G" update-index --force-remove "$2"
        shift 2
        ;;
    esac
  done
  tree=$(git -C "$G" write-tree)
  unset GIT_INDEX_FILE
  git -C "$G" commit-tree "$tree" -p "$parent" -m "$msg"
}

# push <name> <app|op> <sha> <branch>: one git push over HTTPS. Kept in $OUT/<name>.txt:
# what the commit changes against its first parent, git's output and its exit code.
push() {
  local name=$1 who=$2 sha=$3 branch=$4 code
  {
    echo "push by $who: $sha -> refs/heads/$branch"
    echo "the commit against its parent (git diff-tree -r --raw --no-renames):"
    git -C "$G" diff-tree -r --raw --no-renames "$sha^" "$sha"
    echo
  } >"$OUT/$name.txt"
  PW=$([ "$who" = app ] && echo "$TOKEN" || echo "$OP_TOKEN") \
    git -C "$G" -c credential.helper='!f() { echo username=x-access-token; echo "password=$PW"; }; f' \
    push --porcelain "$URL" "$sha:refs/heads/$branch" >>"$OUT/$name.txt" 2>&1
  code=$?
  echo "exit=$code" >>"$OUT/$name.txt"
  printf '%-40s %-4s exit=%s %s\n' "$name" "$who" "$code" \
    "$(grep -E '^(remote: error|!|\*|=| )' "$OUT/$name.txt" | grep -v '^ *$' | tail -1 | cut -c1-170)"
  sleep 1
}

# landed <name> <branch>: where the branch is afterwards, read with the App's token.
landed() {
  call "$1-ref" app "$V1" GET "repos/$R/git/ref/heads/$2"
}

ref_body() { jq -cn --arg r "refs/heads/$1" --arg s "$2" '{ref: $r, sha: $s}'; }

# ---- The two branches of the operator's that cases build on.
WFBASE=$(commit "$BASE" "Probe P-5, App part: a base that holds a workflow" file .github/workflows/p5app-base.yml "$(workflow p5app-base)")
push 00a-operator-base op "$WFBASE" probe/p-5-app-base
OPWF=$(commit "$BASE" "Probe P-5, App part: a workflow that the operator pushed" file .github/workflows/p5app-op.yml "$(workflow p5app-op)")
push 00b-operator-workflow op "$OPWF" probe/p-5-app-op-wf

# ---- 1. A new branch, made by the push. Parent: the head of main.
new_branch() { # <number> <case> <operation>...
  local n=$1 c=$2 sha
  shift 2
  sha=$(commit "$BASE" "Probe P-5, App part: $c" "$@")
  push "1-$n-$c" app "$sha" "probe/p-5-app-$c"
  landed "1-$n-$c" "probe/p-5-app-$c"
}
new_branch 01 control file p5app/control.txt "control"
new_branch 02 add file .github/workflows/p5app-add.yml "$(workflow p5app-add)"
new_branch 03 nested file sub/.github/workflows/p5app-nested.yml "$(workflow p5app-nested)"
new_branch 04 case file .GitHub/workflows/p5app-case.yml "$(workflow p5app-case)"
new_branch 05 case-dir file .github/Workflows/p5app-case-dir.yml "$(workflow p5app-case-dir)"
new_branch 06 link-workflows link .github/workflows ../ci file ci/p5app-link-workflows.yml "$(workflow p5app-link-workflows)"
new_branch 07 link-github rm .github/ISSUE_TEMPLATE/factory-task.yml link .github gh \
  blob gh/ISSUE_TEMPLATE/factory-task.yml "$FORM" file gh/workflows/p5app-link-github.yml "$(workflow p5app-link-github)"
new_branch 08 link-file link .github/workflows/p5app-link-file.yml ../../ci/p5app-link-file.yml \
  file ci/p5app-link-file.yml "$(workflow p5app-link-file)"
new_branch 09 not-yaml file .github/workflows/README.md "Not a workflow."
new_branch 10 github-other file .github/p5app-other.yml "$(workflow p5app-other)"
new_branch 11 deeper file .github/workflows/sub/p5app-deeper.yml "$(workflow p5app-deeper)"
new_branch 12 workflows-file file .github/workflows "A regular file where the directory would be."
new_branch 13 empty-file file .github/workflows/p5app-empty.yml ""

# ---- 2. A new branch on the operator's base, which holds a workflow.
on_base() { # <number> <case> <operation>...
  local n=$1 c=$2 sha
  shift 2
  sha=$(commit "$WFBASE" "Probe P-5, App part: $c" "$@")
  push "2-$n-$c" app "$sha" "probe/p-5-app-$c"
  landed "2-$n-$c" "probe/p-5-app-$c"
}
WFBLOB=$(git -C "$G" rev-parse "$WFBASE:.github/workflows/p5app-base.yml")
on_base 01 base-control file p5app/control.txt "control, on a base that holds a workflow"
on_base 02 base-modify file .github/workflows/p5app-base.yml "$(workflow p5app-base-modified)"
on_base 03 base-delete rm .github/workflows/p5app-base.yml
on_base 04 base-rename rm .github/workflows/p5app-base.yml blob .github/workflows/p5app-renamed.yml "$WFBLOB"
on_base 05 base-mode exec .github/workflows/p5app-base.yml "$(workflow p5app-base)"
on_base 06 base-move-out rm .github/workflows/p5app-base.yml blob ci/p5app-base.yml "$WFBLOB"

# ---- 3. A branch that the App created through the REST API at the head of main, as
#         the claim does (8.2), and then pushes to, as publish does.
existing() { # <number> <case> <operation>...
  local n=$1 c=$2 sha
  shift 2
  call "3-$n-$c-create" app "$V1" POST "repos/$R/git/refs" "$(ref_body "probe/p-5-app-$c" "$BASE")"
  sha=$(commit "$BASE" "Probe P-5, App part: $c" "$@")
  push "3-$n-$c" app "$sha" "probe/p-5-app-$c"
  landed "3-$n-$c" "probe/p-5-app-$c"
}
existing 01 upd-control file p5app/control.txt "control, on an existing branch"
existing 02 upd-add file .github/workflows/p5app-upd-add.yml "$(workflow p5app-upd-add)"
existing 03 upd-link-workflows link .github/workflows ../ci file ci/p5app-upd-link-workflows.yml "$(workflow p5app-upd-link-workflows)"
existing 04 upd-link-file link .github/workflows/p5app-upd-link-file.yml ../../ci/p5app-upd-link-file.yml \
  file ci/p5app-upd-link-file.yml "$(workflow p5app-upd-link-file)"
existing 05 upd-case file .GitHub/workflows/p5app-upd-case.yml "$(workflow p5app-upd-case)"
# Two commits in one push: the first adds a workflow and the second removes it.
call 3-06-upd-add-remove-create app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-upd-add-remove "$BASE")"
C1=$(commit "$BASE" "Probe P-5, App part: upd-add-remove, 1 of 2" file .github/workflows/p5app-gone.yml "$(workflow p5app-gone)" file p5app/control.txt "control")
C2=$(commit "$C1" "Probe P-5, App part: upd-add-remove, 2 of 2" rm .github/workflows/p5app-gone.yml)
push 3-06-upd-add-remove app "$C2" probe/p-5-app-upd-add-remove
landed 3-06-upd-add-remove probe/p-5-app-upd-add-remove

# ---- 4. The REST API in place of a push.
b64() { printf '%s' "$1" | openssl base64 -A; }
# 4a. The contents endpoint.
call 4a-01-contents-branch app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-contents "$BASE")"
call 4a-02-contents-control app "$V1" PUT "repos/$R/contents/p5app/contents.txt" \
  "$(jq -cn --arg c "$(b64 control)" '{message: "Probe P-5, App part: contents, control", branch: "probe/p-5-app-contents", content: $c}')"
call 4a-03-contents-workflow app "$V1" PUT "repos/$R/contents/.github/workflows/p5app-contents.yml" \
  "$(jq -cn --arg c "$(b64 "$(workflow p5app-contents)")" '{message: "Probe P-5, App part: contents, a workflow", branch: "probe/p-5-app-contents", content: $c}')"
call 4a-04-contents-nested app "$V1" PUT "repos/$R/contents/sub/.github/workflows/p5app-contents-nested.yml" \
  "$(jq -cn --arg c "$(b64 "$(workflow p5app-contents-nested)")" '{message: "Probe P-5, App part: contents, a nested workflow", branch: "probe/p-5-app-contents", content: $c}')"
landed 4a-05-contents probe/p-5-app-contents

# 4b. The git data endpoints: a blob, a tree, a commit, and then a ref at the commit.
call 4b-01-blob app "$V1" POST "repos/$R/git/blobs" "$(jq -cn --arg c "$(workflow p5app-gitdata)" '{content: $c, encoding: "utf-8"}')"
DBLOB=$(jq -r '.sha // empty' <<<"$BODY")
call 4b-02-tree app "$V1" POST "repos/$R/git/trees" \
  "$(jq -cn --arg base "$(git -C "$G" rev-parse "$BASE^{tree}")" --arg sha "$DBLOB" \
    '{base_tree: $base, tree: [{path: ".github/workflows/p5app-gitdata.yml", mode: "100644", type: "blob", sha: $sha}]}')"
DTREE=$(jq -r '.sha // empty' <<<"$BODY")
call 4b-03-commit app "$V1" POST "repos/$R/git/commits" \
  "$(jq -cn --arg t "$DTREE" --arg p "$BASE" '{message: "Probe P-5, App part: gitdata, a workflow", tree: $t, parents: [$p]}')"
DCOMMIT=$(jq -r '.sha // empty' <<<"$BODY")
echo "gitdata: blob=$DBLOB tree=$DTREE commit=$DCOMMIT"
if [ -n "$DCOMMIT" ]; then
  call 4b-04-ref-create app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-gitdata "$DCOMMIT")"
  call 4b-05-ref-branch app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-gitdata-upd "$BASE")"
  call 4b-06-ref-update app "$V1" PATCH "repos/$R/git/refs/heads/probe/p-5-app-gitdata-upd" "$(jq -cn --arg s "$DCOMMIT" '{sha: $s}')"
  landed 4b-07-gitdata probe/p-5-app-gitdata
  landed 4b-08-gitdata-upd probe/p-5-app-gitdata-upd
fi

# 4c. A commit that is in the repository already, pushed by the operator, which adds
#     a workflow: a ref created at it, a ref moved to it, a push of it, a merge of it.
call 4c-01-ref-at-operator app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-ref-at-op "$OPWF")"
call 4c-02-move-branch app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-move-to-op "$BASE")"
call 4c-03-move-to-operator app "$V1" PATCH "repos/$R/git/refs/heads/probe/p-5-app-move-to-op" "$(jq -cn --arg s "$OPWF" '{sha: $s}')"
push 4c-04-push-at-operator app "$OPWF" probe/p-5-app-push-at-op
landed 4c-04-push-at-operator probe/p-5-app-push-at-op
call 4c-05-merge-branch app "$V1" POST "repos/$R/git/refs" "$(ref_body probe/p-5-app-merge "$BASE")"
call 4c-06-merge-control app "$V1" PUT "repos/$R/contents/p5app/merge.txt" \
  "$(jq -cn --arg c "$(b64 control)" '{message: "Probe P-5, App part: merge, a commit of its own", branch: "probe/p-5-app-merge", content: $c}')"
call 4c-07-merge app "$V1" POST "repos/$R/merges" '{"base":"probe/p-5-app-merge","head":"probe/p-5-app-op-wf","commit_message":"Probe P-5, App part: merge of a branch that adds a workflow"}'
landed 4c-08-merge probe/p-5-app-merge
# A child of the operator's commit that leaves the workflow as it is.
CHILD=$(commit "$OPWF" "Probe P-5, App part: on-op-control" file p5app/control.txt "control, on the operator's commit")
push 4c-09-on-operator-control app "$CHILD" probe/p-5-app-on-op-control
landed 4c-09-on-operator-control probe/p-5-app-on-op-control

# ---- The branches as they are, for the record.
call 90-branches app "$V1" GET "repos/$R/git/matching-refs/heads/probe/p-5-app"
jq -r '.[] | "\(.object.sha[0:7]) \(.ref)"' <<<"$BODY" | tee "$OUT/90-branches-short.txt"
