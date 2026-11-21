#!/bin/bash
# Probe P-15, first part: everything the operator's token writes before the workflow runs.
#   targets   the issues that the text refers to and tries to close
#   payload   the text of every case, written by gen.py from the targets' numbers
#   pulls     fourteen branches of one empty commit each, and a pull request from each:
#             seven created with the text as their body, seven whose body the workflow sets
#   workflow  the branch probe/p-15 with probe-p15.yml, bot.sh and the payload, which starts the run
# The second round, after the first was read: targets2, payload again, operator, and workflow second.
# The third round, after the second was read: targets3, payload again, operator3.
# The fourth round, after the third was read: targets4, payload again, and workflow fourth.
# Usage: setup.sh <results-dir> targets|payload|pulls|targets2|operator|targets3|operator3|targets4
#        setup.sh <results-dir> workflow text|commits|second|fourth
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/lib.sh"

mkdir -p "$1/setup"
OUT=$(cd "$1" && pwd)

target() {  # target <name> <title>
  rest "$OUT/setup/$1.txt" POST issues -f title="P-15 probe: target, $2" \
    -f body="A target of probe P-15. Text elsewhere refers to this issue. Nothing here is a request."
  printf '%s\t%s\n' "$1" "$(field "$OUT/setup/$1.txt" .number)" >> "$OUT/ids.tsv"
  sleep 1
}

case $2 in
targets)
  { gh --version | head -1; echo "api-version=$V"; date -u +%Y-%m-%dT%H:%M:%SZ; } > "$OUT/00-inputs.txt"
  : > "$OUT/ids.tsv"
  target t-hash "the form #n"
  target t-gh "the form GH-n"
  target t-full "the form owner/repo#n"
  target t-url "the form of a URL"
  target t-title "a title"
  target t-close-comment "a closing keyword in a comment"
  target t-close-body "a closing keyword in an issue body"
  for e in $ENCODINGS; do
    target "t-close-prc-$e" "a closing keyword in a pull request body at creation, $e"
    target "t-close-pre-$e" "a closing keyword in an edited pull request body, $e"
    target "t-close-cbody-$e" "a closing keyword in a commit body, $e"
    [ "$e" = fence ] || target "t-close-csubj-$e" "a closing keyword in a commit subject, $e"
  done
  ;;
targets2)
  for v in v-noscheme v-nohost v-zwsp-number v-zwsp-issues v-zwsp-owner v-lower-gh; do
    target "t-$v" "another spelling, $v"
  done
  target t-op-empty "the operator's empty commit"
  target t-op-file "the operator's commit of a file"
  ;;
operator)
  # Two commits by the operator on a branch that is never merged: does a commit that only
  # refers to an issue leave an event there, when the workflow's commits left none?
  W=$(mktemp -d)
  git clone --quiet "https://github.com/$REPO.git" "$W/s"
  git -C "$W/s" switch --quiet -c probe/p-15-operator
  git -C "$W/s" commit --quiet --allow-empty --cleanup=verbatim -F "$OUT/payload/operator-empty.txt"
  echo "A file of probe P-15." > "$W/s/p15-operator.txt"
  git -C "$W/s" add p15-operator.txt
  git -C "$W/s" commit --quiet --cleanup=verbatim -F "$OUT/payload/operator-file.txt"
  git -C "$W/s" push --quiet origin probe/p-15-operator
  git -C "$W/s" log --format='operator-commit=%H %an %s' -2 | tee -a "$OUT/00-inputs.txt"
  rm -rf "$W"
  ;;
targets3)
  target t-v-zwsp-both "another spelling, v-zwsp-both"
  target t-op-as-bot "a commit in the bot's name that the operator pushed"
  printf 't-v-pull\t%s\n' "$(id "$OUT" pr-prc-fence)" >> "$OUT/ids.tsv"   # an existing pull request, not a new issue
  ;;
operator3)
  # The third round, all of it the operator's. The workflow's commits left no `referenced` event
  # and the operator's two did, so the same messages are committed again by the operator, on the
  # branch that is never merged. One more commit carries the bot's name and the operator's push.
  # And two more spellings of a reference go into a comment.
  W=$(mktemp -d)
  git clone --quiet "https://github.com/$REPO.git" "$W/s"
  git -C "$W/s" switch --quiet probe/p-15-operator
  for e in $ENCODINGS; do
    for part in csubj cbody; do
      [ -f "$OUT/payload/$part-$e.txt" ] || continue
      git -C "$W/s" commit --quiet --allow-empty --cleanup=verbatim -F "$OUT/payload/$part-$e.txt"
    done
  done
  git -C "$W/s" commit --quiet --allow-empty --cleanup=verbatim -F "$OUT/payload/variants-commit.txt"
  BOT='github-actions[bot]'; MAIL='41898282+github-actions[bot]@users.noreply.github.com'
  GIT_AUTHOR_NAME=$BOT GIT_AUTHOR_EMAIL=$MAIL GIT_COMMITTER_NAME=$BOT GIT_COMMITTER_EMAIL=$MAIL \
    git -C "$W/s" commit --quiet --allow-empty --cleanup=verbatim -F "$OUT/payload/operator-as-bot.txt"
  git -C "$W/s" push --quiet origin probe/p-15-operator
  git -C "$W/s" log --format='operator-commit=%H %an %s' -15 | cut -c1-110 | tee -a "$OUT/00-inputs.txt"
  rm -rf "$W"
  rest "$OUT/setup/more-spellings.issue.txt" POST issues -f title="P-15 probe: source, comment, more spellings" \
    -f body="An issue of probe P-15. Its one comment holds the text."
  sleep 1
  rest "$OUT/setup/more-spellings.comment.txt" POST "issues/$(field "$OUT/setup/more-spellings.issue.txt" .number)/comments" \
    -F body=@"$OUT/payload/variants-more.md"
  ;;
targets4)
  for v in v-ref-num-raw v-ref-hex-raw v-ref-num-z v-ref-hex-z v-ref-dec-z v-ref-amp; do
    target "t-$v" "a character reference, $v"
  done
  target t-c-fence-broken "a fence that the text ends"
  target t-c-fence-long "a longer fence around the same text"
  ;;
payload)
  python3 "$HERE/gen.py" "$OUT"
  ls "$OUT/payload" | wc -l
  ;;
pulls)
  MAIN=$(gh api "repos/$REPO/git/ref/heads/main" --jq .object.sha)
  TREE=$(gh api "repos/$REPO/git/commits/$MAIN" --jq .tree.sha)
  echo "main=$MAIN" >> "$OUT/00-inputs.txt"
  for kind in prc pre; do
    for e in $ENCODINGS; do
      b=probe/p-15-$kind-$e
      rest "$OUT/setup/pr-$kind-$e.commit.txt" POST git/commits -f tree="$TREE" -f "parents[]=$MAIN" \
        -f message="P-15 probe: an empty commit for the pull request $kind, $e"
      rest "$OUT/setup/pr-$kind-$e.ref.txt" POST git/refs -f ref="refs/heads/$b" \
        -f sha="$(field "$OUT/setup/pr-$kind-$e.commit.txt" .sha)"
      if [ $kind = prc ]; then
        rest "$OUT/setup/pr-$kind-$e.txt" POST pulls -f base=main -f head="$b" \
          -f title="P-15 probe: source, pull request body at creation, $e" -F body=@"$OUT/payload/prc-$e.md"
      else
        rest "$OUT/setup/pr-$kind-$e.txt" POST pulls -f base=main -f head="$b" \
          -f title="P-15 probe: source, pull request body edited, $e" \
          -f body="A pull request of probe P-15. The workflow replaces this body."
      fi
      printf '%s\t%s\n' "pr-$kind-$e" "$(field "$OUT/setup/pr-$kind-$e.txt" .number)" >> "$OUT/ids.tsv"
      sleep 1
    done
  done
  ;;
workflow)
  W=$(mktemp -d)
  git clone --quiet "https://github.com/$REPO.git" "$W/s"
  git -C "$W/s" switch --quiet probe/p-15 2>/dev/null || git -C "$W/s" switch --quiet -c probe/p-15
  mkdir -p "$W/s/.github/workflows" "$W/s/p15"
  cp "$HERE/probe-p15.yml" "$W/s/.github/workflows/"
  cp "$HERE/bot.sh" "$OUT/ids.tsv" "$W/s/p15/"
  cp -R "$OUT/payload" "$W/s/p15/"
  echo "$3" > "$W/s/p15/part"
  git -C "$W/s" add -A
  git -C "$W/s" commit --quiet -m "Probe P-15: write the $3 as github-actions[bot]"
  git -C "$W/s" push --quiet origin probe/p-15
  echo "workflow-commit-$3=$(git -C "$W/s" rev-parse HEAD)" | tee -a "$OUT/00-inputs.txt"
  rm -rf "$W"
  ;;
esac
