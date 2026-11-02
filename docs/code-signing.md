# Code signing policy

Every file of a release is built from this repository by
[`.github/workflows/release.yml`](../.github/workflows/release.yml) on GitHub Actions, when a
`release/<version>` branch is pushed, and uploaded to a draft GitHub Release that is published by
hand. Nothing is built or signed on a personal computer.

## Signatures

| Files | Signed with |
|---|---|
| Windows setup `.exe` and `codelibre_text.exe` | Not signed yet. Applying for: free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org) |
| macOS `.dmg` | Ad hoc only (no Apple Developer ID yet): open it with right click > Open |
| Linux `.deb` and `.rpm` | The release GPG key |
| `SHA256SUMS` (the checksums of every file) | The release GPG key, in `SHA256SUMS.asc` |
| Every file | A [build provenance attestation](https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations) of the workflow run that built it |

The release GPG key is [`packaging/codelibre-text-release.asc`](../packaging/codelibre-text-release.asc):

```
Oscar Chacon Cautin (CodeLibre Text releases) <oscarchaconcautin@gmail.com>
RSA 4096, fingerprint 379D 0C0C 47F1 19EB D9D7  2D93 2E99 B167 E0E2 DFC6
```

Its private key exists only in the maintainer's keyring and in the repository's Actions secrets.

### Verifying a download

```sh
# The checksums are the ones the release key signed:
gpg --import codelibre-text-release.asc
gpg --verify SHA256SUMS.asc SHA256SUMS
# The file is the one listed:
sha256sum --check --ignore-missing SHA256SUMS      # macOS: shasum -a 256 --check --ignore-missing SHA256SUMS
# It was built by this repository's workflow (GitHub CLI):
gh attestation verify codelibre-text-<version>-linux-x64.tar.xz --repo oscarchacon/codelibre_text
```

The `.rpm` signature: `sudo rpm --import codelibre-text-release.asc && rpm --checksig <file>.rpm`.

## Team roles

CodeLibre Text has one maintainer, who holds every role:

- Committers and reviewers: [oscarchacon](https://github.com/oscarchacon) — pull requests from anyone
  else are reviewed before they are merged.
- Approvers: [oscarchacon](https://github.com/oscarchacon) — every release is approved by hand
  (merging into `release/<version>`, then publishing the draft release).

Team members use multi-factor authentication on GitHub (and on SignPath).

## Privacy policy

This program will not transfer any information to other networked systems unless specifically
requested by the user or the person installing or operating it. It only connects to the network
when asked to: Tools > Install Package Control downloads Package Control from GitHub, and
Help > Report an Issue opens GitHub in the browser with a report the user reviews before sending
it. Packages and plugins the user installs (Package Control included) are third-party code with
their own behavior and privacy policies.
