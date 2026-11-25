# 17 - The scratch App

Recorded 2026-11-25. Resolves issue #81: which App id, slug and installation id the scratch App has, and where its private key lies (SPEC GH-11a, GH-11b, D-38). The spec does not change: GH-11a keeps the name of both Apps out of it, and this report is where the scratch App's identifiers are read from.

Method: the operator created the App by hand in GitHub's settings on 2026-11-25, from the checklist of #81, and gave the session the App id, the slug, the installation id and the path of the key file. The session then read the App back from GitHub, authenticated as the App: a JSON Web Token signed with the key file by `openssl` 4.1.0, valid for five minutes, sent with `curl` on six `GET` requests (section 3). The key was read by `openssl` alone and was not printed. No installation token was created, and nothing was written to GitHub. The first installation token is the work of #82.

The repository is public. The identifiers below are not secrets: an App's id and slug show on every comment and check run it writes. The key is not in this repository and its content is nowhere in this report. A statement marked **(inference)** is a reading of a response, not something the response shows.

## 1. Answer

| The ticket asks | Answer |
|---|---|
| App id | 5926866 |
| Slug | `sf-scratch-app` (the name is the same) |
| Installation id | 185898834, on the account `sapientpants` |
| Path of the key file | `~/Projects/sf-scratch-app/sf-scratch-app.2026-11-25.private-key.pem` on the operator's Mac, mode 0600 |

## 2. The checklist of #81 against what GitHub returned

| Step | Asked | Observed |
|---|---|---|
| 1 | Owned by the account `sapientpants` | `GET /app`: `owner.login` is `sapientpants`. Created 2026-11-25T20:22:12Z |
| 3 | No webhook | `GET /app`: `events` is empty. `GET /app/hook/config` returned 404. That the 404 means an inactive webhook is an **(inference)**; the settings page was not read by the session |
| 4 | The seven permissions of SPEC 5.7 and no other | `GET /app`: `actions: read`, `checks: write`, `contents: write`, `issues: write`, `metadata: read`, `pull_requests: write`, `statuses: read`, and no other key. `GET /app/installations/185898834` returned the same seven. Neither holds `workflows` or `administration` (GH-11) |
| 5 | Installable on this account only | Not observed. No field of the six responses says it; it is step 5 of the checklist, done by the operator |
| 6 | The key outside every repository, mode 0600 | The directory `~/Projects/sf-scratch-app` is not inside a git work tree. The file was mode 0644 as downloaded, and the session set it to 0600 on 2026-11-25. The directory is mode 0755 |
| 7 | Installed on `sapientpants`, on `sf-scratch` only | `GET /app`: `installations_count` is 1. `GET /app/installations`: one installation, 185898834, `repository_selection: selected`, not suspended, created 2026-11-25T20:23:26Z. `GET /repos/sapientpants/sf-scratch/installation` returned that installation. `GET /repos/sapientpants/software-factory/installation` returned 404. The operator states that the installation has access to `sf-scratch` and to no other repository |

That the key file belongs to this App follows from the first request: GitHub accepted a token whose issuer is 5926866 and whose signature the file made.

## 3. The requests

All six to `https://api.github.com`, with `Accept: application/vnd.github+json` and `X-GitHub-Api-Version: 2022-11-28`:

| Request | Status |
|---|---|
| `GET /app` | 200 |
| `GET /app/hook/config` | 404 |
| `GET /app/installations` | 200 |
| `GET /app/installations/185898834` | 200 |
| `GET /repos/sapientpants/sf-scratch/installation` | 200 |
| `GET /repos/sapientpants/software-factory/installation` | 404 |

With the operator's OAuth token, as `gh` holds it, `GET /apps/sf-scratch-app` returned 404 and `GET /user/installations` returned 403, `You must authenticate with an access token authorized to a GitHub App`. Neither read is open to that token for this App.

## 4. What was not observed

- **Which repositories the installation covers, as a list.** That needs an installation token (`GET /installation/repositories`). What was seen is that the selection is `selected`, that `sf-scratch` is in it and that the factory repository is not. A third repository of the account would not have shown. That there is none is the operator's statement of 2026-11-25, read from the settings page, and not an observation of the session. #82 can list them with its first token.
- **That the App is installable on the operator's account only** (step 5).
- **The settings page.** Every observation is a response of the REST API.
- **Anything the App can do.** No call was made under an installation token, so nothing here says what the seven permissions allow or refuse. Those are the probes of #82.
- **A ruleset bypass list** (GH-11). The scratch repository has no ruleset on 2026-11-25 (`GET /repos/sapientpants/sf-scratch/rulesets`, on the operator's token, returned an empty list), so there is no list for the App to be on. The ruleset on `factory/**` that #82 asks about does not exist yet.
