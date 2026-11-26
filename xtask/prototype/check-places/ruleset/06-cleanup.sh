#!/bin/sh
# Removes the two rulesets and the four branches. The pull requests stay, merged.
. "$(dirname "$0")/lib.sh"
for id in $(gh api $R/rulesets --jq '.[] | select(.name|startswith("p38-")) | .id'); do
  call "delete ruleset $id" -X DELETE $R/rulesets/$id >/dev/null
done
for n in a b c base; do
  call "delete branch probe/p-38-$n" -X DELETE $R/git/refs/heads/probe/p-38-$n >/dev/null
done
call "rulesets left" $R/rulesets --jq 'length'
