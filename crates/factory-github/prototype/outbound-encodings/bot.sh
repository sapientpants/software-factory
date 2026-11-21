#!/bin/bash
# Probe P-15, second part: runs in the workflow, so every write here is made
# by github-actions[bot] and a mention of the operator is a mention by another account.
# For each encoding: an issue with the text in one comment, an issue with the
# text as its body, an issue with the text in its title, and the text set as
# the body of a pull request that the operator opened. Then one commit with
# the text in its subject and one with the text in its body, pushed to a branch.
# `second` and `fourth` are later rounds, each written after the one before was read.
# Usage: bot.sh text|commits|second|fourth   (in a clone of the scratch repository, with GH_TOKEN and REPO set)
set -u
cd "$(dirname "$0")"
v='X-GitHub-Api-Version: 2026-03-10'
show='{number, title, user: .user.login, created_at, updated_at}'

text() {
for e in plain fence code backslash zwj zwsp entity; do
  echo "== $e: an issue, then the text as its one comment"
  n=$(gh api --method POST "repos/$REPO/issues" -H "$v" -f title="P-15 probe: source, comment, $e" \
    -f body="An issue of probe P-15. Its one comment holds the text." --jq .number)
  echo "comment-$e=$n"
  sleep 2
  gh api --method POST "repos/$REPO/issues/$n/comments" -H "$v" -F body=@"payload/comment-$e.md" \
    --jq '{id, user: .user.login, created_at}'
  sleep 2

  echo "== $e: an issue with the text as its body"
  gh api --method POST "repos/$REPO/issues" -H "$v" -f title="P-15 probe: source, body, $e" \
    -F body=@"payload/body-$e.md" --jq "$show"
  sleep 2

  if [ -f "payload/title-$e.txt" ]; then
    echo "== $e: an issue with the text in its title"
    gh api --method POST "repos/$REPO/issues" -H "$v" -f title="$(cat "payload/title-$e.txt")" \
      -f body="An issue of probe P-15. Its title holds the text." --jq "$show"
    sleep 2
  fi

  echo "== $e: the text as the new body of the operator's pull request"
  pr=$(awk -F'\t' -v k="pr-pre-$e" '$1 == k { print $2 }' ids.tsv)
  gh api --method PATCH "repos/$REPO/pulls/$pr" -H "$v" -F body=@"payload/pre-$e.md" --jq "$show"
  sleep 2
done
}

commits() {
echo "== the commits"
git config user.name 'github-actions[bot]'
git config user.email '41898282+github-actions[bot]@users.noreply.github.com'
tree=$(git rev-parse 'origin/main^{tree}')
head=$(git rev-parse origin/main)
for e in plain fence code backslash zwj zwsp entity; do
  for part in csubj cbody; do
    [ -f "payload/$part-$e.txt" ] || continue
    # The message as it is in the file. Not with -F: commit-tree opens that path from the top of the work tree.
    head=$(git commit-tree "$tree" -p "$head" < "payload/$part-$e.txt") || exit 1
    echo "$part-$e=$head"
  done
done
git push --quiet origin "$head:refs/heads/probe/p-15-commits"
git log --format='%H %an <%ae> %s' "origin/main..$head"
}

# The second round. The factory edits its status comment in place, so: does text
# that arrives by an edit act as text that was there from the start?
second() {
for e in plain fence code backslash zwj zwsp entity; do
  echo "== $e: an issue and a comment, then the text as the comment's new body"
  n=$(gh api --method POST "repos/$REPO/issues" -H "$v" -f title="P-15 probe: source, comment edited, $e" \
    -f body="An issue of probe P-15. Its one comment is edited to hold the text." --jq .number)
  echo "cedit-$e=$n"
  sleep 2
  c=$(gh api --method POST "repos/$REPO/issues/$n/comments" -H "$v" \
    -f body="A comment of probe P-15. The workflow replaces this text." --jq .id)
  sleep 2
  gh api --method PATCH "repos/$REPO/issues/comments/$c" -H "$v" -F body=@"payload/comment-$e.md" \
    --jq '{id, user: .user.login, created_at, updated_at}'
  sleep 2
done

echo "== plain: an issue, then the text as its new body"
n=$(gh api --method POST "repos/$REPO/issues" -H "$v" -f title="P-15 probe: source, body edited, plain" \
  -f body="An issue of probe P-15. The workflow replaces this body." --jq .number)
sleep 2
gh api --method PATCH "repos/$REPO/issues/$n" -H "$v" -F body=@payload/body-plain.md --jq "$show"
sleep 2

echo "== other spellings of a reference: in a comment, and in a commit on a branch"
n=$(gh api --method POST "repos/$REPO/issues" -H "$v" -f title="P-15 probe: source, comment, other spellings" \
  -f body="An issue of probe P-15. Its one comment holds the text." --jq .number)
sleep 2
gh api --method POST "repos/$REPO/issues/$n/comments" -H "$v" -F body=@payload/variants.md \
  --jq '{id, user: .user.login, created_at}'
git config user.name 'github-actions[bot]'
git config user.email '41898282+github-actions[bot]@users.noreply.github.com'
head=$(git commit-tree "$(git rev-parse 'origin/main^{tree}')" -p "$(git rev-parse origin/main)" < payload/variants-commit.txt) || exit 1
echo "variants-commit=$head"
git push --quiet origin "$head:refs/heads/probe/p-15-variants"
}

# The fourth round: character references, and a fence that the text ends.
# One issue for each case, with the text as its body.
fourth() {
for f in payload/case-*.md; do
  name=${f#payload/case-}; name=${name%.md}
  echo "== $name: an issue with the text as its body"
  gh api --method POST "repos/$REPO/issues" -H "$v" -f title="P-15 probe: source, body, case $name" \
    -F body=@"$f" --jq "$show"
  sleep 2
done
echo "== a reference spelt with a character reference: in a comment"
n=$(gh api --method POST "repos/$REPO/issues" -H "$v" -f title="P-15 probe: source, comment, character-reference spellings" \
  -f body="An issue of probe P-15. Its one comment holds the text." --jq .number)
sleep 2
gh api --method POST "repos/$REPO/issues/$n/comments" -H "$v" -F body=@payload/variants-charref.md \
  --jq '{id, user: .user.login, created_at}'
}

"$1"
