# SignPath integration and approval setup

**Status:** the workflow and artifact configuration are prepared. No SignPath application, account configuration or signing request has been submitted by this work. Production signing remains disabled until approved and configured. The public v0.1.3 download is still unsigned.

Use [signpath_application.md](signpath_application.md) as the owner-review packet and application checklist. The public [Code signing policy](code_signing_policy.md) lists responsibilities and privacy behaviour.

## Flow

```text
Selected release tag
  → tests and packaged application smoke check on GitHub-hosted Windows
  → unsigned distribution ZIP + checksum, and executable-only GitHub artifact
  → protected release-signing environment
  → SignPath verifies the executable artifact's GitHub build provenance
  → manual approval in SignPath
  → download signed executable
  → check product/version, trusted publisher signature and timestamp
  → smoke test the signed executable with the original bundled runtime
  → rebuild ZIP, verify every packaged file and regenerate SHA-256 checksum
  → create/refresh draft release, or publish only if explicitly requested
```

The API token is a submission credential, not an exported signing key. The approver reviews requests in SignPath. Fork and pull-request builds do not enter this signing flow. A rejection, timeout, cancellation, missing setting or signature failure blocks the signed release; there is no unsigned fallback when SignPath is enabled.

The current workflow waits up to one hour for SignPath completion, within a 90-minute job. Review the request in SignPath while the job waits. Request details are available in the action logs and, when returned by the action, the GitHub job summary. If it expires or the run is cancelled, reject/cancel the stale request and rerun deliberately; approving an old request does not resume a completed GitHub job.

## Configure after Foundation acceptance

1. Complete the [SignPath Foundation application](https://signpath.org/apply) and follow their onboarding instructions. Approval is external and is not guaranteed by the prepared files.
2. Enable SignPath MFA for the maintainer. GitHub MFA has been confirmed by the owner; SignPath MFA is not yet confirmed.
3. Configure the project's GitHub.com trusted build system and, as requested, install/authorize the SignPath GitHub App for `Ala-Ismail/12daz-App`. All jobs leading to a request must use GitHub-hosted runners, as this workflow does.
4. Create the SignPath project and import [signpath-artifact.xml](../signing/signpath-artifact.xml). Validate it in SignPath's editor. It expects the GitHub ZIP wrapper containing exactly the executable path `12dAZ-Converter.exe`; it is **not** configured for a nested distribution ZIP.
5. Set up the production signing policy with the Foundation-provided certificate and timestamping. Enable **manual approval** and designate `Ala-Ismail` as approver. Restrict the CI account/token to submission for this project/policy; do not grant it approval rights.
6. Restrict SignPath's pipeline/origin policy to the official repository, reviewed workflow and intended release source. Ask SignPath to verify the selected source commit when a manual run on `main` builds a different release tag; do not assume the workflow ref and checkout ref are identical.
7. In GitHub, create/protect the **release-signing** environment. Restrict eligible branches/tags to `main` and intended `v*` release tags; protect tag creation, workflows and signing configuration. Enable environment approval where appropriate. SignPath's own manual approval remains required regardless of GitHub reviewer settings.
8. Add the environment configuration below. Populate real values supplied by SignPath; do not guess a production policy slug or certificate publisher.

| Type | Name | Value |
| --- | --- | --- |
| Environment secret | `SIGNPATH_API_TOKEN` | Submission-only SignPath CI/API token |
| Environment variable | `SIGNPATH_ORGANIZATION_ID` | SignPath organization ID |
| Environment variable | `SIGNPATH_PROJECT_SLUG` | Approved project slug |
| Environment variable | `SIGNPATH_SIGNING_POLICY_SLUG` | Manually approved production signing policy slug |
| Environment variable | `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` | Slug of the imported executable artifact configuration |
| Environment variable | `SIGNPATH_PUBLISHER` | Exact common name of the granted signing certificate; expected Foundation identity, not automatically the app company's name |

The token must be a secret of the **release-signing environment**, including when using the reusable workflow via **Publish release**. It is consumed by the environment-bound job and is not passed through reusable-workflow inputs. Never put it into YAML, XML, documentation, application text or issue comments.

9. Set repository-level `SIGNING_ENABLED=false` (Azure switch), then `SIGNPATH_ENABLED=true`. Both switches must be unset, `false` or `true`; enabling both fails the build. Keep `SIGNPATH_ENABLED` unset/false until all external setup is complete.

## Artifact identity

The PyInstaller recipe embeds product name **12dAZ Converter**, company **Lambda Technologies Pty Ltd**, original filename **12dAZ-Converter.exe** and file/product versions derived from the application version. SignPath's XML checks these fields, including the request's `version` parameter. It signs only the project executable, without wildcard signing of Python or third-party DLLs.

The build uploads that executable independently from the distribution ZIP and passes its immutable GitHub artifact ID to SignPath. After approval, `scripts/package_signpath_release.ps1` inserts the verified signed executable into the original ZIP contents, preserving runtime files. SHA-256 checksums are calculated again after signing and packaging.

Changes to the XML in Git alone do not update SignPath's server-side configuration; import and review the matching version there. Foundation review must confirm whether this PyInstaller executable/bootloader packaging is eligible and what upstream-code conditions apply.

## First signed release

Use a **new** version/tag whose source includes the Windows metadata changes. Old v0.1.3 binaries lack the new metadata and will fail the restrictions. Existing published assets are never replaced by this workflow.

1. Update application and project versions together, commit the intended release source, and create its new matching tag through the existing release process. Do not move existing tags.
2. Run **Build and release** on `main` with the new existing tag and leave `publish_release=false` to prepare a draft first.
3. Review the SignPath request's tag/source commit, artifact identity, metadata and build evidence, then manually approve that request in SignPath.
4. Check the returned signature, smoke-test result, final ZIP/checksum and draft notes. Download and extract the draft asset yourself; verify its signature again and perform clean-machine checks.
5. Publish the reviewed draft manually. Update website/README download links, version and signing status together; only then make the SignPath acknowledgement unconditional for signed releases.

The general **Publish release** workflow passes `publish_release=true`; it can publish after successful signing approval. Use the draft route above for initial qualification. Changing `publish-release.yml` has an existing automatic release trigger; no changes to that file are required just to enable SignPath.

## Verification limits

Local checks can validate workflow syntax, XML structure, Windows metadata, packaging rejection paths and application builds. They cannot establish Foundation acceptance, server-side artifact/pipeline policies, live approval or a trusted signing success without an approved SignPath account. Record the first successful request/build evidence before claiming live signing is verified.

Preparation evidence is recorded in [validation_status.md](validation_status.md#signpath-preparation-checks--15-november-2026): 79 application/metadata tests passed, the actual Windows package built and smoke-tested successfully, XML passed the published schema, and unsigned/misconfigured requests were rejected. Packaging after the signature boundary was checked with a signature test double; no trust store was changed and no real signed release was produced.

References: [official GitHub integration](https://docs.signpath.io/trusted-build-systems/github), [artifact configuration syntax](https://docs.signpath.io/artifact-configuration/syntax), [file metadata restrictions](https://docs.signpath.io/artifact-configuration/reference#file-metadata-restrictions), [Foundation terms](https://signpath.org/terms).
