#!/bin/sh
# Probe P-1: what does POST /git/refs return for a ref that already exists,
# and how does that differ from the call's other failures?
# Runs against the scratch repository only.
# Usage: probe.sh <output-dir> [issue-number] [api-version]
set -u

REPO=sapientpants/sf-scratch
OUT=$1
N=${2:-9001}                      # issue number used in the ref names
V=${3:-2026-03-10}                # REST API version (GH-17 pins 2026-03-10)
mkdir -p "$OUT"

MAIN=$(gh api "repos/$REPO/git/ref/heads/main" --jq .object.sha)
OTHER=$(gh api "repos/$REPO/git/ref/heads/probe/p-16" --jq .object.sha)
TREE=$(gh api "repos/$REPO/git/commits/$MAIN" --jq .tree.sha)
BLOB=$(gh api "repos/$REPO/git/trees/$TREE" --jq '[.tree[] | select(.type == "blob")][0].sha')
ABSENT=0123456789abcdef0123456789abcdef01234567
{
  echo "main=$MAIN"; echo "other=$OTHER"; echo "tree=$TREE"; echo "blob=$BLOB"; echo "absent=$ABSENT"
  gh --version | head -1
  date -u +%Y-%m-%dT%H:%M:%SZ
} > "$OUT/00-inputs.txt"

# create <case> <ref> <sha>: one POST, the whole response kept
create() {
  gh api -i --method POST "repos/$REPO/git/refs" \
    -H "X-GitHub-Api-Version: $V" \
    -f ref="$2" -f sha="$3" > "$OUT/$1.txt" 2>&1
  printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$(head -1 "$OUT/$1.txt" | tr -d '\r')"
}

R="refs/heads/factory/issue-$N"

create a1-first              "$R"                                "$MAIN"
create a2-again-same-sha     "$R"                                "$MAIN"
create a3-again-other-sha    "$R"                                "$OTHER"
create a4-again-absent-sha   "$R"                                "$ABSENT"
create a5-again-short-sha    "$R"                                "abc123"

create b1-absent-sha         "refs/heads/factory/issue-$((N+1))" "$ABSENT"
create b2-short-sha          "refs/heads/factory/issue-$((N+1))" "abc123"
create b3-tree-sha           "refs/heads/factory/issue-$((N+1))" "$TREE"
create b4-blob-sha           "refs/heads/factory/issue-$((N+1))" "$BLOB"
create b5-no-refs-prefix     "heads/factory/issue-$((N+1))"      "$MAIN"
create b6-bare-name          "factory/issue-$((N+1))"            "$MAIN"
create b7-invalid-name       "refs/heads/factory/issue..$((N+1))" "$MAIN"
create b8-name-with-space    "refs/heads/factory/issue $((N+1))" "$MAIN"
create b9-below-a-branch     "$R/sub"                            "$MAIN"
create b10-above-a-branch    "refs/heads/factory"                "$MAIN"

# no sha field at all
gh api -i --method POST "repos/$REPO/git/refs" -H "X-GitHub-Api-Version: $V" \
  -f ref="refs/heads/factory/issue-$((N+1))" > "$OUT/b11-no-sha.txt" 2>&1
printf 'b11-no-sha\t\t\t%s\n' "$(head -1 "$OUT/b11-no-sha.txt" | tr -d '\r')"

# no credential
curl -sS -i -X POST "https://api.github.com/repos/$REPO/git/refs" \
  -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: $V" \
  -d "{\"ref\":\"$R\",\"sha\":\"$MAIN\"}" > "$OUT/b12-no-credential.txt" 2>&1
printf 'b12-no-credential\t\t\t%s\n' "$(head -1 "$OUT/b12-no-credential.txt" | tr -d '\r')"

# reads that step 3 of 8.2 needs after a refused create
for pair in "c1-get-existing:heads/factory/issue-$N" \
            "c2-get-prefix-of-existing:heads/factory/issue-$((N/10))" \
            "c3-get-absent:heads/factory/issue-$((N+500))"; do
  name=${pair%%:*}; path=${pair#*:}
  gh api -i "repos/$REPO/git/ref/$path" -H "X-GitHub-Api-Version: $V" > "$OUT/$name.txt" 2>&1
  printf '%s\t%s\t\t%s\n' "$name" "$path" "$(head -1 "$OUT/$name.txt" | tr -d '\r')"
done

# race: two creates of one new ref at once, 5 rounds with the same sha and 5 with different shas
i=0
while [ $i -lt 10 ]; do
  ref="refs/heads/factory/issue-$((N+100+i))"
  if [ $i -lt 5 ]; then second=$MAIN; else second=$OTHER; fi
  create "r$i-x" "$ref" "$MAIN" &
  create "r$i-y" "$ref" "$second" &
  wait
  gh api "repos/$REPO/git/ref/${ref#refs/}" --jq '"  ref now at " + .object.sha' | tee "$OUT/r$i-after.txt"
  i=$((i+1))
done
