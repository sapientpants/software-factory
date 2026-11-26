#!/bin/sh
# Case 2: both rulesets. The second has no bypass actor and requires p38/ci.
. "$(dirname "$0")/lib.sh"
call "create ruleset p38-ci" -X POST $R/rulesets --input "$(dirname "$0")/ruleset-ci.json" --jq '{id,name,current_user_can_bypass,bypass_actors}'
call "rules for the base, both rulesets" $R/rules/branches/probe/p-38-base --jq '.[] | {type, ruleset_id, parameters}'
call "open PR b" -X POST $R/pulls -f title="P-38: head b" -f head=probe/p-38-b -f base=probe/p-38-base -f body="Probe for sapientpants/software-factory issue 87." --jq '{number}'
