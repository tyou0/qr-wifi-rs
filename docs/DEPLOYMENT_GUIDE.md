# Deployment and Store Publication Guide

This document is both the project release runbook and a study guide for designing
multi-channel software delivery. It distinguishes **artifact creation** from
**publication**: producing a ZIP, installer, or DMG does not mean a package
manager or app store has accepted it.

## 1. Delivery architecture

```text
Developer on clean main
        |
        | ./scripts/release-all.sh X.Y.Z
        v
Canonical Gitea main + annotated vX.Y.Z tag
        |
        | configured push mirror (branches + tags)
        v
Public GitHub mirror
        |
        | tag-triggered GitHub Actions
        v
Cross-platform build matrix
        |
        +--> GitHub Release + SHA256SUMS                [automated]
        +--> Homebrew personal tap formula update       [post-tag/manual today]
        +--> Chrome Web Store submission                [not configured]
        +--> Firefox AMO submission                     [not configured]
        +--> WinGet manifest PR                         [not configured]
        +--> Microsoft Store submission                 [not configured]
        +--> Apple signing/notarization or App Store    [not configured]
```

### Why Gitea is canonical

The repository of record is the `gitea` remote. Release code pushes `main` and
the annotated tag atomically to Gitea. A server-side push mirror copies those
refs to GitHub. Contributors must not bypass this chain by pushing releases
directly to GitHub, because doing so creates two competing histories and makes
release provenance ambiguous.

GitHub is the public build and distribution forge. Its larger hosted-runner
matrix produces Linux, Windows, macOS ARM64, and macOS Intel artifacts.

## 2. Current deployment status

| Channel | Built automatically | Published automatically | Signing/trust state |
| --- | --- | --- | --- |
| GitHub Release | Yes, on mirrored `v*` tag | Yes | SHA-256 manifest; binaries otherwise unsigned except ad-hoc macOS signature |
| Linux AppImage/DEB/RPM | Yes | GitHub Release only | Unsigned |
| Windows NSIS/MSI | Yes | GitHub Release only | Unsigned |
| macOS app/DMG | Yes | GitHub Release only | Ad-hoc signed; not Developer ID signed or notarized |
| CLI/TUI/native host archives | Yes | GitHub Release | SHA-256 manifest |
| Browser extension ZIP | Yes | GitHub Release | Unsigned; not submitted to Chrome Web Store or Firefox AMO |
| Homebrew personal tap | Formula is maintained here | Post-tag tap update is currently manual | Public tag archive + pinned SHA-256 |
| Homebrew Core | No | No | Not submitted |
| WinGet | No manifest yet | No | Not submitted |
| Microsoft Store | No Store submission lane; existing EXE/MSI must be assessed against unpackaged Win32 policy | No | Unsigned; not submitted |
| Apple Mac App Store | No store-compatible package | No | Current networking model may conflict with sandbox rules |

The distinction matters: this repository is already a complete example of
cross-platform **release artifact automation**, but store publication remains a
separate, credentialed phase.

## 3. Version authority and synchronization

A release has one stable `X.Y.Z` semantic version represented in three files.
The release command intentionally rejects prerelease/build suffixes because Chrome
extension manifest versions accept only one to four dot-separated integers; a
separate prerelease mapping policy would otherwise be required.

1. `Cargo.toml` — Rust workspace packages.
2. `src-tauri/tauri.conf.json` — desktop bundle metadata.
3. `extension/manifest.json` — browser-visible extension version.

`scripts/release-all.sh` updates all three in one local transaction.
`scripts/test-release-packaging.sh` rejects version drift. This prevents a
correctly tagged extension archive from advertising an older version.

The stable Homebrew formula is deliberately **not** required to match the
workspace version during the pre-tag release gate. Its immutable source archive
and checksum do not exist until the public tag is available. Homebrew publication
is therefore a post-tag phase.

## 4. Supported release procedure

### Preconditions

- Checkout is on `main` with no tracked or untracked changes.
- Local `HEAD`, `gitea/main`, and `origin/main` are identical.
- `gitea` is the canonical remote; `origin` is the GitHub mirror.
- The version and tag do not exist locally or on either forge.
- Gitea's push mirror is healthy and configured to synchronize tags.
- Required CI runners are available.

### Release

```sh
./scripts/release-all.sh X.Y.Z
```

The script:

1. Validates semantic version syntax and required commands.
2. Verifies branch, clean-worktree, remote, and forge synchronization gates.
3. Rejects an existing local, Gitea, or GitHub tag.
4. Requires an interactive publication confirmation.
5. Updates Cargo, Tauri, and extension versions.
6. Refreshes `Cargo.lock` and runs release contracts/tests.
7. Creates one release commit and annotated tag.
8. Atomically pushes `main` and `vX.Y.Z` to Gitea.
9. Leaves GitHub synchronization to the configured Gitea mirror.

Before publication, failure restores the original local commit and removes the
local tag. After publication, never rewrite the tag: fix forward with a new
patch release.

### Release acceptance

Do not call a release successful until all of these are true:

- Gitea `main`, GitHub `main`, and the intended release commit match.
- The peeled annotated tag points to the release commit on both forges.
- Every GitHub build-matrix job is successful.
- Every expected uniquely named artifact exists exactly once.
- `SHA256SUMS.txt` verifies every release file.
- At least one artifact per platform family is opened or executed on its target OS.
- Package-manager channels are tested independently after publication.

## 5. GitHub Release automation

`.github/workflows/release.yml` has four phases:

1. **CLI/TUI/native host matrix** — builds three binaries on four native runners.
2. **Desktop matrix** — builds AppImage/DEB/RPM, NSIS/MSI, and app/DMG bundles.
3. **Extension packaging** — lints and creates an unsigned extension ZIP.
4. **Release publication** — downloads artifacts, generates SHA-256 checksums,
   and creates the GitHub Release.

Artifact names include component, version, OS, and architecture. Collection
requires exactly one match per expected bundle. This prevents silent overwrites
or uploads of stale output.

The macOS collection code intentionally supports Apple Bash 3.2; do not use
`mapfile` or `readarray` in cross-platform Bash steps.

## 6. Homebrew personal tap

### Current design

The canonical formula exists in two synchronized copies:

- `Formula/qr-wifi-rs.rb`
- `scripts/homebrew-formula.rb`

The public tap is `tyou0/homebrew-qr-wifi-rs`. Stable builds use a public GitHub
tag archive and an exact SHA-256. They must never depend on an inaccessible
private Gitea archive.

### Post-tag publication

1. Wait for the mirrored public tag.
2. Download the exact GitHub tag archive.
3. Compute SHA-256 locally.
4. Update both checked-in formula copies.
5. Run formula syntax and packaging contracts.
6. Update the separate tap repository.
7. Test a clean target-Mac installation or upgrade.
8. Verify CLI, TUI, native host, GUI binary, and desktop app version/provenance.

Homebrew publication is separate from source release because the checksum is a
property of the already-published archive.

### Future automation pattern

Use a post-release workflow triggered only after the GitHub Release succeeds.
It should check out the tap with a narrowly scoped token, update URL/checksum,
run `brew audit` and installation tests on macOS, then open a reviewable pull
request. Avoid directly mutating tap `main` from an unreviewed build job.

Suggested secret:

- `HOMEBREW_TAP_TOKEN` — fine-grained access to only the tap repository.

## 7. Browser extension stores

The extension depends on a separately installed Native Messaging host. Store
listings and onboarding must explain that requirement and provide per-browser
host-manifest installation instructions.

### Chrome Web Store blueprint

1. Create and verify a Chrome Web Store developer account.
2. Reserve a stable extension item ID.
3. Put that ID in the native-host `allowed_origins` configuration.
4. Build the exact reviewed extension ZIP.
5. Authenticate to the current Chrome Web Store API using the store's supported
   OAuth/service credential flow.
6. Upload to the existing item, then explicitly publish or submit for review.
7. Poll processing/review state and record the store version and item URL.
8. Install from the public store and test through the real native host.

Suggested protected secrets/variables:

- `CHROME_EXTENSION_ID`
- OAuth client/service credentials required by the current Web Store API
- A refresh token if the selected API flow requires one

Never print tokens, upload responses containing credentials, or expose Native
Messaging host paths from a private workstation.

### Firefox Add-ons (AMO) blueprint

The manifest already has a stable Gecko ID. Publication can use `web-ext sign`
with AMO JWT credentials:

- `AMO_JWT_ISSUER`
- `AMO_JWT_SECRET`

Use the listed channel for AMO review/public distribution. After signing, retain
the signed XPI as an immutable artifact and install it in a clean Firefox
profile. Validate Native Messaging through the signed add-on, not only a
temporary extension.

### Browser-store release gate

- Manifest version equals source release version.
- Lint has zero errors and zero policy warnings treated as errors.
- Permissions are minimal and listing privacy declarations match implementation.
- Store item IDs match native-host allowlists.
- Signed/store-delivered extension performs a real request/response with the host.
- Rollback uses a new accepted version or store rollback controls; never reuse a
  published extension version.

## 8. WinGet

WinGet is manifest-driven; a GitHub installer does not automatically appear in
WinGet.

### Required package inputs

- Stable package identifier, for example `Tyou.QRWiFiRS` after ownership checks.
- Versioned HTTPS installer URL from the immutable GitHub Release.
- Installer type (`wix` for MSI or `nullsoft` for NSIS, based on the chosen file).
- SHA-256 of the exact installer.
- Architecture, locale, publisher, license, product code/upgrade behavior, and
  silent-install switches as applicable.

### Publication blueprint

1. Publish and verify the Windows installer first.
2. Generate or update manifests using Microsoft's current WinGetCreate tooling.
3. Validate manifests locally with the current WinGet validation command.
4. Test clean install, upgrade, uninstall, PATH/shortcut behavior, and version.
5. Open a PR to `microsoft/winget-pkgs` from an automation identity or reviewed bot.
6. Monitor validation and moderation; do not report publication until the package
   is searchable from a clean WinGet client.

Store a WinGet token only if automation opens PRs:

- `WINGET_GITHUB_TOKEN` — fine-grained access needed for the manifest fork/PR.

A package identifier must be researched before creation; do not assume a short
name is available or owned by this project.

## 9. Windows signing and Microsoft Store

Current NSIS/MSI artifacts are unsigned. Production distribution should use a
trusted code-signing certificate or a managed signing service. Signing must
occur after build and before checksum generation.

Suggested protected secrets depend on the selected provider:

- Certificate/PFX secret and password, or
- Cloud signing account, certificate profile, tenant/client identity, and OIDC.

Prefer keyless/managed signing with short-lived federation over exporting a
long-lived private key to CI.

Microsoft Store offers two materially different desktop lanes:

1. **Unpackaged Win32 listing.** A qualifying existing EXE or MSI can be submitted
   through Partner Center without first converting it to MSIX. The installer must
   meet current Store policies, support silent/reliable install and uninstall,
   avoid unsupported bootstrap behavior, use stable public installer URLs, and
   pass clean-machine, upgrade, and uninstall validation. This project's NSIS EXE
   or MSI may be candidates only after signing and policy/behavior verification;
   their existence alone does not make them Store-ready.
2. **Packaged MSIX lane.** Add MSIX identity, capabilities, signing, update, and
   Partner Center metadata when Store-managed package identity or MSIX features
   are desired. This is a separate packaging output, not a relabeled NSIS/MSI.

Both lanes require Partner Center ownership, listing/privacy metadata, submission
credentials, and Store-delivered acceptance testing. Choose deliberately based on
application capabilities and current Microsoft policy; do not claim MSIX is the
only route or claim an ordinary release installer is already Store-compatible.

## 10. macOS signing, notarization, and Mac App Store

Current macOS bundles are ad-hoc signed. That verifies internal consistency but
does not establish developer identity or Gatekeeper trust.

### Direct-download production lane

1. Sign nested binaries and app with a Developer ID Application identity.
2. Build/sign the DMG or ZIP without changing the signed app afterward.
3. Submit to Apple's notarization service.
4. Wait for acceptance and staple the ticket where supported.
5. Run `codesign --verify`, `spctl --assess`, and a clean-Mac launch test.
6. Generate checksums only after signing/notarization is complete.

Typical protected inputs:

- Base64 signing certificate/P12 and password, or secure signing integration
- `APPLE_ID`, app-specific password, and team ID, or App Store Connect API key
- `APPLE_TEAM_ID`, signing identity, and bundle identifier variables

### Mac App Store constraints

Mac App Store delivery is not merely a notarized DMG. It requires App Store
Connect records, distribution signing, provisioning, sandbox-compatible
entitlements, privacy declarations, and store review. This app invokes OS Wi-Fi
management tools; verify that the required behavior is compatible with the App
Sandbox before promising a Mac App Store build. If it is not, keep the
Developer-ID-notarized direct-download lane as the supported macOS channel.

## 11. Linux repositories

Current Linux packages are release assets, not apt/dnf repository publications.
A production repository lane needs:

- Repository metadata generation.
- GPG signing keys held outside ordinary build jobs.
- Version/architecture naming policy.
- Atomic metadata upload.
- Clean apt/dnf installation and upgrade tests.

AppImage can remain a direct-download channel. Flathub/Snapcraft are separate
reviewed ecosystems with their own manifests, sandbox permissions, and tokens.

## 12. Secrets and trust boundaries

| Boundary | Principle |
| --- | --- |
| Gitea → GitHub mirror | Server-owned credential; developers do not manually push release refs to GitHub |
| Build jobs | Read-only source by default; no store credentials |
| GitHub Release publisher | `contents: write` only in the final release job |
| Store publishers | Separate environments with approvals and channel-specific secrets |
| Signing | Sign immutable reviewed output; checksum afterward |
| Homebrew tap | Fine-grained access to tap only; preferably PR-based |
| External manifest repos | Bot token limited to fork/PR operations |

Use GitHub Environments for production channels, required reviewers, branch/tag
restrictions, and secret isolation. Pin third-party actions to immutable commit
SHAs for a hardened production pipeline; version tags are easier to read but are
mutable references.

## 13. Store-publishing workflow design

Keep build and publish as separate jobs:

```text
build -> test -> package -> sign -> checksum -> release
                                      |
                                      v
                           approved channel publishers
```

Publishers should consume immutable artifacts from the successful release, not
rebuild source independently. Each publisher must be idempotent: detect an
already-published version and exit safely rather than creating duplicates.

Recommended workflow inputs:

- `version`
- `channel` (`chrome`, `firefox`, `homebrew`, `winget`, `microsoft-store`,
  `apple-notary`, etc.)
- `dry_run` defaulting to true

Require explicit environment approval when `dry_run=false`.

## 14. Rollback and incident response

- **Git/source:** revert on `main`; never rewrite a published tag.
- **GitHub Release:** preserve provenance; publish a fixed patch instead of
  replacing assets under the same version.
- **Homebrew:** update formula to a known-good immutable tag/checksum and test.
- **Browser stores:** use store rollback controls where available or submit a new
  higher version; browser stores generally reject version reuse.
- **WinGet:** submit a corrected higher-version manifest or approved manifest fix.
- **Signed desktop apps:** revoke only when key compromise demands it; otherwise
  publish a fixed signed version.

For signing-key or token compromise: disable the publisher, rotate credentials,
audit store/forge activity, document affected versions, and re-establish trust
before resuming automated publication.

## 15. Adding a new distribution channel

Use this checklist as the reusable pattern:

1. Identify authoritative registry/store and prove package-name ownership.
2. Define immutable input artifact and version mapping.
3. Document required metadata, signing, permissions, and review policy.
4. Build a dry-run validator without production credentials.
5. Add contract tests for filenames, version synchronization, and metadata.
6. Put publication credentials in a protected environment, never general CI.
7. Publish only after release acceptance and human approval.
8. Test install, launch/help, upgrade, uninstall, and provenance on a clean target.
9. Record store URL/item ID and rollback procedure.
10. Keep the status matrix honest: “artifact built” is not “store published.”

## 16. Reference files

- `.gitea/workflows/ci.yml` — canonical-forge Linux CI.
- `.github/workflows/ci.yml` — cross-platform CI and package contracts.
- `.github/workflows/release.yml` — native build matrix and GitHub Release.
- `scripts/release-all.sh` — guarded Gitea-first release transaction.
- `scripts/test-release-packaging.sh` — executable packaging/version contract.
- `Formula/qr-wifi-rs.rb` — Homebrew formula.
- `extension/manifest.json` — browser extension identity/version.
- `src-tauri/tauri.conf.json` — desktop bundle identity/version.
- `docs/CI_CD.md` — concise operator summary.
- `docs/SECURITY.md` — application security model.
