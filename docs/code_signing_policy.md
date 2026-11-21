# Code signing policy

12dAZ Converter is an MIT-licensed Windows desktop application maintained by Lambda Technologies Pty Ltd. Its source and release builds are hosted in [Ala-Ismail/12daz-App](https://github.com/Ala-Ismail/12daz-App).

## Current status

The project is preparing for the SignPath Foundation open-source signing programme. The signing workflow and artifact configuration are prepared, with application, account configuration and activation pending. The published v0.4.0 Windows package is unsigned. A matching checksum verifies download integrity. Publisher signatures are verified separately.

The Azure signing integration described in [code_signing.md](code_signing.md) is a separate, inactive option; it does not provide SignPath access. Future release notes will identify the signing status of each release.

Maintainer preparation: [SignPath setup](signpath_setup.md) and [application review packet](signpath_application.md). These files do not authorize signing or claim external acceptance.

If SignPath approves the project and signing is activated, the following acknowledgement will apply:

> Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

This is prospective attribution, not a claim of current sponsorship.

## Team and release authorization

| Role | Responsible member |
| --- | --- |
| Author / committer | Repository owner [Ala-Ismail](https://github.com/Ala-Ismail) |
| Reviewer | Repository owner [Ala-Ismail](https://github.com/Ala-Ismail) |
| Signing approver | Repository owner [Ala-Ismail](https://github.com/Ala-Ismail), once signing is activated |

External contributions require maintainer review. Each SignPath signing request will require manual approval by the designated approver. Public access, forks and pull requests do not authorize use of this project's signing identity.

The repository owner has confirmed that multi-factor authentication is enabled for their GitHub account. SignPath MFA and release/workflow protections remain pending confirmation and must be in place before SignPath signing is activated.

## Artifacts and verification

Only official 12dAZ Converter release artifacts built from the public repository's identified source commit will be submitted. The project will not sign unrelated software or upstream runtime libraries as its own code. Packaged third-party components retain their own licences and any upstream signatures.

SignPath configuration must enforce the project's product name and release version metadata. Tests, packaged-app checks, signature/timestamp verification and final ZIP checksums must pass before a release is described as signed. Existing unsigned releases will remain labelled as such. A failed signing attempt must not produce a release described as signed.

Users can obtain official packages and checksums from [GitHub Releases](https://github.com/Ala-Ismail/12daz-App/releases). To inspect an extracted executable:

```powershell
Get-AuthenticodeSignature -LiteralPath '.\12dAZ-Converter.exe' |
    Format-List Status, StatusMessage, SignerCertificate, TimeStamperCertificate
```

A signature identifies the signing publisher and protects executable integrity. It does not certify conversion fidelity or guarantee that every new release will avoid SmartScreen reputation warnings.

## Privacy and network behaviour

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

Survey files are processed locally. The current converter has no accounts, telemetry, automatic update checks or survey-upload service. DXF files and conversion reports are written locally to the selected destination.

The optional support link opens PayPal only when clicked. Requests made by the browser are subject to [PayPal's privacy statement](https://www.paypal.com/privacy). Downloading software, viewing GitHub or visiting the website involves those services separately from offline conversion; see [GitHub's privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement) and [Vercel's privacy policy](https://vercel.com/legal/privacy-policy) for website hosting where deployed on Vercel.

Signing submits build artifacts to the selected signing provider; it does not submit users' survey files. Never attach private survey data to public issues. Non-sensitive questions about this policy can be raised in [GitHub Issues](https://github.com/Ala-Ismail/12daz-App/issues).

Programme requirements: [SignPath Foundation terms](https://signpath.org/terms). This policy must be updated when signing status, team membership or network behaviour changes.
