#!/bin/sh
# Case 1, continued: a direct update of the base, then the merge of the pull request.
. "$(dirname "$0")/lib.sh"
a=$(gh api $R/git/ref/heads/probe/p-38-a --jq .object.sha)
call "direct update of the base to head a, gates alone" -X PATCH $R/git/refs/heads/probe/p-38-base -f sha=$a --jq '{message,status,ref}'
call "PR $1 before the merge" $R/pulls/$1 --jq '{mergeable,mergeable_state,head:.head.sha}'
call "GraphQL view of PR $1" graphql -f query='query($n:Int!){repository(owner:"sapientpants",name:"sf-scratch"){pullRequest(number:$n){mergeStateStatus reviewDecision viewerCanMergeAsAdmin statusCheckRollup{state}}}}' -F n=$1 --jq .data.repository.pullRequest
call "merge PR $1 over REST, gates alone" -X PUT $R/pulls/$1/merge -f merge_method=squash --jq '{merged,message,status}'
