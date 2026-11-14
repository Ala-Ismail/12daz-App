# Build and release on GitHub

The **Build and release** workflow in `.github/workflows/test.yml` runs on GitHub's Windows x64 runner. It installs pinned dependencies, runs the tests, builds the standalone package, verifies the frozen app and produces a ZIP with a SHA-256 checksum.

## Download a build

1. Push your source commit to `main`, or open a pull request.
2. Open the repository's **Actions** tab and select **Build and release**.
3. Open a successful run and download its Windows artifact from **Artifacts**.
4. Extract the artifact wrapper, then extract the application ZIP inside it. Run `12dAZ-Converter.exe` with `_internal` beside it.

Artifacts are retained for 30 days. Release attachments remain available with their release. A build from a branch or pull request does not create a release.

## Prepare a release for an existing tag

The existing `v0.1.2` tag predates this workflow update. Keep the tag unchanged:

1. Commit and push the workflow update to `main`.
2. In **Actions → Build and release → Run workflow**, choose branch `main`.
3. Enter `v0.1.2` in **release_tag**, then select **Run workflow**.
4. The workflow checks out the tagged source, tests and builds it, and creates a **draft pre-release** with the ZIP and `SHA256SUMS.txt` attached.
5. Open **Releases**, review the draft and select **Publish release** when ready.

Leaving `release_tag` blank performs a build only. The supplied tag must already exist and must match both the application and `pyproject.toml` versions. For example, tag `v0.1.2` requires version `0.1.2`.

To publish after verification, check **publish_release** when running the workflow. Alternatively, use **Actions > Publish release > Run workflow**, enter the existing tag and run it. That release-specific workflow builds the exact tagged source and publishes the verified ZIP and checksum as a pre-release. A push changing that release-specific workflow creates its configured new annotated version tag at the triggering commit, then builds and publishes it. Existing tags are never moved. The current configured version is v0.1.3; manual runs require an existing tag.

A rerun can refresh attachments on an existing draft. An already published release is preserved; the new package remains available as an Actions artifact. Use a new version/tag for a revised published package.

## Future versions

1. Update the versions in `src/az12_converter/__init__.py` and `pyproject.toml` together, and update relevant release documentation.
2. Commit and push the change.
3. Create and push a new annotated tag matching the version, for example:

```powershell
git tag -a v0.1.3 -m "12dAZ Converter v0.1.3"
git push origin v0.1.3
```

Pushing the tag runs the workflow and prepares the draft automatically. The filenames are derived from the application version; the workflow does not require a filename edit for each release.

## Permissions and failure behaviour

The build job uses read-only repository access. The release job uses GitHub's provided `GITHUB_TOKEN` with `contents: write`; no personal token needs to be added. Repository or organisation policy must allow that workflow permission.

Release preparation runs only after tests, packaging, packaged-app verification and checksum verification succeed. Ordinary tag builds leave drafts; the explicit publish option or release-specific workflow publishes after those checks. Neither workflow moves tags or replaces published assets. Build failures appear in Actions, with available packaging diagnostics attached.

Independent CAD interoperability, clean-machine installation and remaining release gates in [test requirements](test_requirements.md) still need qualification. Passing this workflow does not imply those manual checks have passed.
