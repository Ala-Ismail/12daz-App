# SignPath Foundation application preparation

**For owner review; not submitted.** Verify account settings and any requested private contact details in the application portal. Do not publish an API token, private key, survey file or identity document in this repository.

## Project details for the application

| Field | Prepared value |
| --- | --- |
| Project | 12dAZ Converter |
| Maintainer organization | Lambda Technologies Pty Ltd, Australia; ABN 22 647 165 160 |
| Repository | https://github.com/Ala-Ismail/12daz-App |
| Maintainer / GitHub owner | https://github.com/Ala-Ismail |
| Licence | MIT; https://github.com/Ala-Ismail/12daz-App/blob/main/LICENSE |
| Releases and existing Windows download | https://github.com/Ala-Ismail/12daz-App/releases |
| Public signing policy | https://github.com/Ala-Ismail/12daz-App/blob/main/docs/code_signing_policy.md |
| Website source | https://github.com/Ala-Ismail/12daz-App/tree/main/website |
| Deployment URL | Owner to supply the actual Vercel/custom-domain URL; do not infer it from repository name |
| Contact email | Owner to supply privately in the SignPath application |
| Authors, reviewers and signing approver | Repository owner `Ala-Ismail`; see the public policy |
| Build | GitHub-hosted Windows runner, Python 3.11, PyInstaller; pinned dependencies |
| Requested signing target | Windows Authenticode signature for `12dAZ-Converter.exe` only |

### Tool description

12dAZ Converter is an engineering utility for surveyors, civil engineers and CAD designers that converts `.12daz` and `.12da` files into editable 2D DXF 2018 drawings. It provides model selection, an interactive plan preview and layer-based export while preserving source XY coordinates and supported circular arcs, with elevations flattened to zero. The application runs offline on Windows without requiring 12d Model, AutoCAD or Python on the user's computer, and generates conversion reports identifying unsupported geometry, omissions and style substitutions for engineering review.

### Purpose of the request

We would like to apply for the SignPath Foundation open-source programme to sign official Windows releases of 12dAZ Converter. The current public v0.1.3 release is unsigned and can trigger Windows publisher/reputation warnings. We have prepared a GitHub-hosted build/signing flow, product/version metadata restrictions and a public code-signing/privacy policy. Production signing will remain disabled until acceptance and configuration. The maintainer has confirmed GitHub MFA and will complete SignPath MFA and manual approval setup before activation.

## Evidence and remaining checks

| Item | Current evidence / action |
| --- | --- |
| Public source and MIT licence | Available in the repository. |
| Existing release | Public v0.1.3 Windows ZIP and checksum exist; remain unsigned. |
| Engineering scope and limitations | README and `docs/validation_status.md`; independent CAD and clean-machine qualification remain pending. |
| GitHub MFA | Confirmed by the repository owner. |
| Public policy and privacy | `docs/code_signing_policy.md` and `website/code-signing-policy.html`; link is present on home/download pages and generated release notes. |
| Workflow and XML | `.github/workflows/test.yml`, `signing/signpath-artifact.xml`, `scripts/package_signpath_release.ps1`; prepared for review, not live signing evidence. |
| SignPath acceptance/account | Pending owner application and Foundation review. |
| SignPath MFA, manual approval, token restrictions | Must be configured/confirmed after onboarding. |
| GitHub release/environment protections | Owner must configure and confirm. |
| Deployed website and current release-page policy link | Owner to verify actual deployed URL and existing published release text; generated notes apply to future releases. |
| Eligibility of bundled code | Ask SignPath to review PyInstaller bootloader/runtime packaging and third-party licensing; do not assume all eligibility requirements are satisfied. |
| Reputation and other application evidence | Provide accurate requested usage/history evidence; do not invent download counts, users, testimonials or endorsements. |

## Review before submitting

- Read the current [Foundation terms](https://signpath.org/terms) and answer the application accurately. A public MIT repository does not guarantee acceptance.
- Confirm the named maintainer accepts author/reviewer/approver responsibilities and can manually approve releases. Explain the current single-maintainer structure if asked.
- Keep sponsorship wording conditional until approval and activation. Existing unsigned downloads must not be described as signed.
- Commit/push this preparation so the referenced files are publicly reviewable, and ensure the website policy page is deployed.
- Submit through [SignPath's application page](https://signpath.org/apply) using the owner's contact information. No application or external message has been sent by preparing this packet.

After acceptance, follow [signpath_setup.md](signpath_setup.md). Do not activate Azure or SignPath merely to demonstrate readiness for the application.
