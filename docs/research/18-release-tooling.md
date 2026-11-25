# 18 - What the release tooling documents

Recorded 2026-11-25, for issue #84: what the book of `cargo-dist` and the manual of `gh` say about the points that the release build rests on (SPEC DEV-12, DEV-13, D-39). Nothing here was observed. No workflow was generated, no release was built and no attestation was verified. What GitHub does with the workflow is probe P-35.

Method: two fetching agents read the pages below on 2026-11-25 and reported what each states, with the sentences they quote. The session did not read the pages itself. A statement marked **(inference)** is a reading and not something a page states.

## 1. How far to trust it

- **One page was read in part.** The configuration reference of `cargo-dist` came back word for word for about the first half. The rest came back as a summary, and what rests on the summary is marked **(summary)**.
- **The book names no version.** It marks single settings "since 0.29.0" or "since 0.31.0". The specification pins `cargo-dist` 0.34 (19.2), and nothing was checked against that version.
- **The pages show `actions/attest`.** The permissions in section 3 are those shown for that action. Whether the workflow that `cargo-dist` 0.34 generates uses it was not checked.

## 2. `cargo-dist`

Pages, under `https://axodotdev.github.io/cargo-dist/book/`: `reference/config.html`, `ci/index.html` (`ci/github.html` redirects there), `ci/customizing.html`, `supplychain-security/attestations/github.html`.

| Point | What the pages state |
|---|---|
| Attestations | "can be enabled by setting `github-attestations = true`". The permissions that the generated workflow then asks for are not stated |
| Targets | The key `targets`. `x86_64-unknown-linux-gnu` is a listed value |
| Runner | The default for x86-64 Linux is `ubuntu-22.04`. `github-custom-runners` sets a label for a target |
| Pinned actions | "Typically, it uses tagged versions". `github-action-commits` (since 0.29.0) names a commit for an action |
| An environment | No key puts a job behind a deployment environment. With `allow-dirty = ["ci"]`, "dist will stop trying to update the file and stop checking if it's out of date", and the reference says "We recommend avoiding setting this" |
| Where it publishes | A GitHub Release. `checksum` defaults to `"sha256"`, which gives a `.sha256` file for each archive. `dist-manifest.json` is "included with your release", a source archive is on by default, and no installer is |
| The trigger | The default tag pattern is not stated. `tag-namespace` changes the expression that tags are matched with **(summary)**, and `dispatch-releases` replaces the tag push with `workflow_dispatch` **(summary)** |
| A build cache | Not stated in what was read. The reference lists a key `cache-builds` whose text was not read |

## 3. `gh attestation`

Pages: `https://cli.github.com/manual/gh_attestation_verify`, `gh_attestation_download` and `gh_attestation_trusted-root` under the same path, and two pages of `https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/`: `verifying-attestations-offline` and `using-artifact-attestations-to-establish-provenance-for-builds`.

| Point | What the pages state |
|---|---|
| A login | Not stated. The command "will attempt to fetch relevant attestations via the GitHub API", and the one login the page names is that of a container registry, for an image. That a read through the API needs a token is likely and not stated **(inference)** |
| Whose artifact | `--owner` or `--repo` is required |
| The signing workflow | `--signer-workflow`: "Enforce that the workflow that signed the attestation matches the provided value." The page advises it: "Ideally, the path of the signer workflow is also validated" |
| The source ref | `--source-ref`: "the git ref associated with the source repository matches" |
| The runner | `--deny-self-hosted-runners`: "Fail verification for attestations generated on self-hosted runners." |
| Offline | A machine with network runs `gh attestation download` for the artifact and `gh attestation trusted-root`. The other machine then needs `gh`, the artifact and both files, and runs `gh attestation verify` with `--bundle` and `--custom-trusted-root`. The trusted root has no expiry date and its keys rotate "a few times per year" |
| Permissions to attest | For a binary: `id-token: write`, `contents: read`, `attestations: write` |

## 4. What follows in the specification

- **The workflow is edited by hand** where `cargo-dist` has no setting, which the pages show for the environment (DEV-12). How the edited file is kept is left to the change that adds it.
- **The runner label and the commits of the actions** are settings of the tool, so the pins of DEV-5 and VAL-6 need no hand edit by these pages.
- **The operator verifies on the Mac** (DEV-13). The pages do not say whether the server could verify without a login, and the offline form needs two files that another machine fetched.
