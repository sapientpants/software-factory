# PROTOTYPE: capture from a hostile work tree

Throwaway code for issue #6 and probe P-7 (SPEC 9.4, WS-9). It is not the
implementation and is never merged; the answer it produced is in
`docs/research/probes.md` on `main`.

`capture_proto.py` builds a mirror, a `host.git` and a workspace as WS-1 and
WS-10 say, makes the workspace hostile in one way per case, and runs the
capture of SPEC 9.4 as four layers: scan, listing check, staging, ingest. It
keeps going past the first rejection, so the output shows what every layer
does with every case. A canary script detects anything that executed.

```sh
python3 capture_proto.py        # on this machine, no sandbox
./linux.sh                      # Debian stable in Docker, no sandbox
SF_PROTO_BWRAP=1 ./linux.sh     # the same, staging under bubblewrap
```

`-v` prints every git command; `-k <text>` runs the cases whose name contains
the text. `results/` holds the three runs the probe answer was written from.

`extra_checks.sh` records four single observations the probe record cites:
the `GIT_ATTR_SOURCE` variable against the `--attr-source` option, `--remove`
against `--force-remove` behind a symlink, `ls-tree -z --format` quoting, and
`core.fsmonitor` of a nested repository under `diff-files`. Its output on
git 2.56.1, 2.47.3 and 2.39.5 is in `results/extra-*.txt`.
