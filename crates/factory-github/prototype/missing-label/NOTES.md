# Notes on the responses

- `probe.sh` printed each command's exit code to the terminal and did not keep it. `gh issue edit` (02) exited with code 1; the reads that got a 404 exited with code 1 as `gh api` does; every other call exited with code 0.
- `Z2-timeline.txt` was read before `other-case.sh` ran. `Z4-timeline-after-other-case.txt` is the same timeline read later, on 2026-11-21, after the labels were deleted and the issue closed. It adds the `labeled` event of 07:32:00, which is for a label that already existed, and the `closed` event.
