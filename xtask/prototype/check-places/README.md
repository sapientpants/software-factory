# Probe P-38: the check command in each place, and a second ruleset

Throwaway scripts and their output for issue #87, run on 2026-11-26. The record
is the section of P-38 in `docs/research/probes.md`.

- `deny-cases.sh` runs `cargo deny` 0.22.1 without the advisory database and
  without a network, on macOS. Its output is `results/deny-cases.txt`.
- `mutants-case/` is a crate with one function under
  `cfg(target_os = "linux")`. `results/mutants-case.txt` is what
  `cargo mutants` 27.1.0 said of it on macOS.
- `ruleset/` holds the six scripts that were run in order against
  `sapientpants/sf-scratch` with the operator's token, and the two rulesets
  they created. `results/ruleset-responses.log` holds every call with its
  status and its body. The third and the fifth script take the number of the
  pull request that the script before them opened.
