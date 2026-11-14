# Windows publisher signing

The release workflow supports Microsoft Azure Artifact Signing (formerly Trusted Signing), using a **Public Trust** certificate profile and GitHub OpenID Connect (OIDC). No private key, PFX file or client secret is stored in the repository.

**Status:** integration is prepared, but signing is not active until the owner completes Azure identity verification and configures GitHub. Existing published v0.1.3 downloads remain unsigned. No signed release has been produced or verified against Azure by this change.

## One-time owner setup

The project is also exploring SignPath Foundation's open-source programme. See the public [Code signing policy](code_signing_policy.md) for current status and responsibilities. SignPath is not integrated by the Azure workflow below; leave `SIGNING_ENABLED` unset/false unless intentionally activating Azure signing. SignPath approval and its separate workflow configuration are still required.

The repository owner has confirmed GitHub MFA is enabled. SignPath MFA and release/workflow protections remain pending confirmation; GitHub MFA alone does not activate signing.

1. Use an Azure subscription and create an Artifact Signing account in a supported service region. This is a paid external service: review the current plan and billing before provisioning.
2. Complete **organization** identity validation for **Lambda Technologies Pty Ltd**, using the legal business details and supporting evidence requested by Azure. Microsoft currently lists Australia as eligible for organization Public Trust validation. Individual validation has different geographic restrictions.
3. Create a **Public Trust** certificate profile linked to the approved identity. Private Trust and test profiles are not substitutes for publicly trusted application distribution.
4. Create a Microsoft Entra application/service principal for GitHub signing. Grant it the **Artifact Signing Certificate Profile Signer** role scoped to this certificate profile, rather than broad subscription permissions.
5. Add a federated credential to that application:
   - Issuer: `https://token.actions.githubusercontent.com`
   - Audience: `api://AzureADTokenExchange`
   - Subject: `repo:Ala-Ismail/12daz-App:environment:release-signing`
6. In GitHub repository Settings, create an environment named **release-signing**. Restrict eligible branches/tags to `main` and intended `v*` release tags. Protect creation of release tags and changes to release workflows. Configure an environment reviewer if appropriate for your repository's release policy.
7. Add the following environment variables under **release-signing**. The IDs are identifiers, not passwords; use GitHub configuration variables, matching the workflow's `vars.*` references.

| Variable | Value |
| --- | --- |
| `AZURE_CLIENT_ID` | Entra signing application's client ID |
| `AZURE_TENANT_ID` | Entra directory tenant ID |
| `AZURE_SUBSCRIPTION_ID` | Subscription containing the signing account |
| `SIGNING_ENDPOINT` | Exact HTTPS regional endpoint shown by the signing account |
| `SIGNING_ACCOUNT_NAME` | Artifact Signing account name |
| `SIGNING_CERTIFICATE_PROFILE` | Approved Public Trust profile name |
| `SIGNING_PUBLISHER` | Exact certificate publisher common name, as approved by Azure; expected to identify Lambda Technologies Pty Ltd |

8. Set the **repository-level** Actions variable `SIGNING_ENABLED` to exactly `true` only when configuration is complete. The job's eligibility check needs this variable before entering the environment, so do not put this switch only at environment scope.

See Microsoft's [setup and current eligibility](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart), [role assignments](https://learn.microsoft.com/en-us/azure/artifact-signing/tutorial-assign-roles) and the official [signing action](https://github.com/Azure/artifact-signing-action).

## Release behaviour

- Branch and pull-request builds remain unsigned development artifacts and have no signing-job OIDC permissions. Release signing runs only for version-tag builds, or a requested release tag from a workflow running on `main`/a version tag, with `SIGNING_ENABLED=true`.
- The build job tests and packages the exact selected source and smoke-tests it. The separate signing job checks the artifact's ZIP checksum and extracts it, authenticates through short-lived OIDC, and signs **only `12dAZ-Converter.exe`**. Third-party Python/runtime DLLs are not re-signed as this publisher.
- The executable is signed with SHA-256 and an RFC 3161 timestamp. The job requires a valid Windows Authenticode signature, a timestamp and an exact match to `SIGNING_PUBLISHER`. It then smoke-tests the signed executable, rebuilds the ZIP, checks that the archived executable is byte-identical to the signed executable, and regenerates the ZIP checksum.
- The signed artifact has a `-signed` suffix in Actions. The release job downloads this artifact and publishes the signed ZIP and its new checksum under the normal asset filenames. Original build artifacts are still unsigned; use the signed artifact or published release for distribution.
- If enabled signing fails, is cancelled or is unexpectedly skipped, publication is blocked. There is no fallback to an unsigned release. When signing is disabled/unset, the existing unsigned build/release path remains available and new release notes state that it is unsigned.
- Existing published release assets are preserved. Use a new version/tag for the first signed release; a rerun will not replace published v0.1.3 assets. Refreshing an existing draft updates its assets and generated release notes together, including whether its executable is signed.
- Do not change `publish-release.yml` merely to activate signing: that file has an existing push trigger that attempts version-tag publication. Configure GitHub variables, then use the manual release workflow for the intended new tag.

Before distributing the first signed version, extract its downloaded ZIP and run:

```powershell
Get-AuthenticodeSignature -LiteralPath '.\12dAZ-Converter.exe' |
    Format-List Status, StatusMessage, SignerCertificate, TimeStamperCertificate
```

Confirm `Valid`, the approved publisher, the timestamp and the ZIP checksum. Run the application on a clean Windows machine. Signing adds publisher identity and integrity evidence; it does not establish conversion fidelity or complete the pending CAD/packaging qualification.

## SmartScreen expectations

Public signing establishes a verifiable publisher. It does not guarantee an immediate removal of reputation warnings for every new executable. See [Microsoft's SmartScreen reputation guidance](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation). Do not describe the existing unsigned download as signed or advise users to disable Windows security.

## Local verification of this integration

Both workflows passed actionlint validation. Their inline PowerShell blocks parsed successfully. The actual configuration block rejected missing settings, and the actual signed-package verification block rejected the existing unsigned v0.1.3 executable before smoke testing, re-packaging or publication. Azure authentication, signing, publisher/timestamp acceptance and the signed-package success path remain untested until account setup is complete.
