#!/bin/sh
# Case 2, continued: the merge without the check, the check reported as a
# commit status by the operator's token, and the merge again.
. "$(dirname "$0")/lib.sh"
b=$(gh api $R/pulls/$1 --jq .head.sha)
call "merge PR $1 over REST, both rulesets, no check" -X PUT $R/pulls/$1/merge -f merge_method=squash --jq '{merged,message,status}'
call "GraphQL merge of PR $1, both rulesets, no check" graphql -f query='mutation($id:ID!){mergePullRequest(input:{pullRequestId:$id,mergeMethod:SQUASH}){pullRequest{merged}}}' -f id="$(gh api $R/pulls/$1 --jq .node_id)" --jq '{data,errors:[.errors[]?.message]}'
call "status p38/ci success on the head of PR $1" -X POST $R/statuses/$b -f state=success -f context=p38/ci --jq '{state,context}'
sleep 3
call "PR $1 with the check" $R/pulls/$1 --jq '{mergeable,mergeable_state}'
call "merge PR $1 over REST, both rulesets, p38/ci green" -X PUT $R/pulls/$1/merge -f merge_method=squash --jq '{merged,message,status}'
