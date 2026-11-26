#!/bin/sh
# Case 1: the ruleset with the admin bypass alone. A pull request of the
# operator's with no approval and no check, merged by the operator.
. "$(dirname "$0")/lib.sh"
call "create ruleset p38-gates" -X POST $R/rulesets --input "$(dirname "$0")/ruleset-gates.json" --jq '{id,name,current_user_can_bypass,bypass_actors}'
call "rules for the base, gates alone" $R/rules/branches/probe/p-38-base >/dev/null
call "open PR a" -X POST $R/pulls -f title="P-38: head a" -f head=probe/p-38-a -f base=probe/p-38-base -f body="Probe for sapientpants/software-factory issue 87." --jq '{number,mergeable_state}'
