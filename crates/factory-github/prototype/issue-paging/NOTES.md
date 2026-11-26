# Notes on the scripts and the responses

Probe P-8, the paging part, for #91: an issue whose edit history, events and comments each hold more than one page of 100. Throwaway code, run against `sapientpants/sf-scratch` only. The record is in `docs/research/probes.md` on `main`.

- `paging.sh` ran once, on 2026-11-26 from 12:29:06 to 12:37:38 UTC. It created issue 133 of the scratch repository, wrote to it, read it and closed it. `run.log` is what it printed.
- `lib.sh` is that of the probes of #82 (`prototype/app-token-probes`), with three more headers kept. The script signs a JSON Web Token with the scratch App's private key, through `openssl`, asks for one installation token restricted to the scratch repository, keeps it in a shell variable and revokes it when it ends. The key is not in this repository. `00-token.json` is the response that held the token, with the token taken out.
- The writes of section 1 are in `1-writes.tsv`, one line each: the name, whose token, the method, the status and GitHub's date. The scratch App's token made the first three, `edit-body-1`, `rename-1` and `comment-1`. The operator's token, as `gh` holds it, made every other write of the script.
- What the issue held when the reads began, at 12:36:03: the text as created and 105 body edits, numbered 1 to 105; 106 rename events; three `labeled` and two `unlabeled` events of `probe:p-8`; 105 comments.
- Every read of sections 2 to 6 was made twice, with the operator's token (`-op`, 12:36:03 to 12:36:38) and with the App's (`-app`, 12:36:40 to 12:37:15). Section 7 is the operator's token alone.
- A response file holds the status line, up to seven headers, a blank line and the body.
- `summary.sh` reads the responses and prints `summary.txt`: one entry for each GraphQL read, the pages of the REST reads, and whether the App's read and the operator's are the same. It wrote nothing to GitHub.
- `7k-delete-comment` deleted the comment `Comment 6.`, the one that the cursor of section 7 names. Issue 133 stays on the scratch repository, closed, with its other 105 comments and the label `probe:p-8`.
