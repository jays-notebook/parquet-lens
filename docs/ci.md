# CI and release triggers

CI runs for pushes to `main` and pull requests targeting `main` only when a
changed path matches an application input. The `changes` job then selects the
affected checks; unrelated test runners are skipped.

| Changed inputs | Checks |
| --- | --- |
| `src/**`, `public/**`, `index.html` | Frontend tests, TypeScript check, Vite build |
| npm manifests/lockfiles, `.npmrc`, Node version files | Frontend |
| TypeScript, Vite, Vitest, Tailwind, PostCSS configuration; root `.env*` files | Frontend |
| `src-tauri/**`, including Rust tests, Parquet fixtures, capabilities, configuration and icons | Backend tests on Windows and macOS |
| Root Cargo configuration/manifests/lockfile and Rust toolchain/format configuration | Backend |
| `.github/workflows/ci.yml` | Both |
| Markdown/MDX, `docs/**`, `README.md`, `LICENSE`, `.gitignore`, `.github/CODEOWNERS`, agent/planning files | None |
| `app-icon.svg`, `components.json`, `scripts/build-installer.ps1`, `.github/workflows/release.yml` | None in test CI; these are authoring or separate release inputs |

Markdown and MDX are excluded even inside source directories. If the application
starts importing these formats, update both the event paths and job filters.
Other source assets and test fixtures remain included regardless of extension.
Mixed changes run the union of the affected checks. Added, modified, deleted,
and renamed paths participate in change detection.

The backend tests use Tauri's development configuration and do not embed the
frontend build, so frontend changes do not require Rust test runners. Frontend
configuration changes run the build as well as tests to validate their effect.

Keep the `push.paths`, `pull_request.paths`, and `changes` filters in
`.github/workflows/ci.yml` synchronized when adding new build inputs. The action
compares the full push with its previous tip on `main`; for PRs it uses the PR
file list, including fork PRs, with read-only permissions.

## Releases

Installer builds remain explicit: a `v*` tag builds Windows and macOS installers
and creates a draft release; a manual run uploads installer artifacts. Ordinary
branch pushes and PRs never start this workflow. Tag pushes do not support
GitHub path filtering, so a deliberately pushed version tag still builds even
when its commit only changes documentation. Manual runs also remain available
for rebuilding the same source.

## Dependency security

CI and releases use Node.js 24.22.1 from `.node-version`, `npm ci`, and locked
Cargo dependencies. The Windows installer script also installs the lockfile
on every build and passes `--locked` to Cargo. The refreshed npm lockfile passes clean installation on
macOS ARM and dependency-resolution dry runs for Windows x64 and macOS Intel;
the platform runners remain the final check for native optional dependencies.

`.github/workflows/security.yml` checks both lockfiles on relevant PRs, pushes
to `main`, manual runs, and daily at 00:00 UTC. It verifies the SHA-256 of the
pinned OSV-Scanner binary, downloads the complete public npm and crates.io
advisory databases, and matches locally. Dependency inventories are not uploaded
to npm audit or the OSV query API. New findings or expired exceptions fail CI.

To run the same check with OSV-Scanner 2.7.0:

```sh
osv-scanner scan source --offline --download-offline-databases \
  --config osv-scanner.toml \
  --lockfile package-lock.json --lockfile src-tauri/Cargo.lock
```

The explicit config path applies the reviewed exceptions to both lockfile
directories. `osv-scanner.toml` excludes two Linux GTK advisory IDs until
2027-01-24: glib `RUSTSEC-2024-0429` (unsound iterator) and proc-macro-error
`RUSTSEC-2024-0370` (unmaintained). Both dependencies are absent from the Windows
and macOS build graphs. Review these exceptions before adding Linux support;
they are not fixes for Linux. No npm vulnerability is excluded.

The 2026-10-28 scan also reported 13 Zstd advisories. These are excluded until
2026-11-27 based on a source and feature-graph review; the affected packages
remain present. Arrow/Parquet 59.3.1 and DataFusion 55.2 still require zstd 0.13,
so a compatible lockfile update cannot remove that copy yet. Async compression
already uses the patched zstd 0.14.3 / zstd-safe 8.2.0.

| RUSTSEC IDs | Current applicability evidence |
| --- | --- |
| 2026-0391 | No borrowed streaming `with_context` reuse; bulk contexts are separate from newly owned read decoders. |
| 2026-0395, 2026-0399, 2026-0404 | Affected APIs require `experimental`, which is disabled. |
| 2026-0396, 2026-0398, 2026-0405 | `seekable` is disabled. |
| 2026-0397 | No `OutBuffer::around_pos` use in locked consumers or zstd 0.13 internals; read output uses initialized slices at position zero. |
| 2026-0400 | Parquet/Arrow contexts use bulk operations only; no stream/one-shot context mixing. |
| 2026-0401 | `zdict_builder` is disabled, so dictionary training is not compiled. |
| 2026-0402, 2026-0406 | `zstdmt` is disabled, and no `NbWorkers` configuration is used. |
| 2026-0403 | Shared C library `legacy` support is disabled; bulk/read decoders are also explicitly unaffected, and the write decoder is unused. |

Both safe wrappers and their shared zstd-sys dependency have only `std` enabled
on macOS ARM/Intel and Windows x64. Backend CI rejects activation of the five
features above. Reassess all Zstd exceptions whenever Zstd, Arrow, Parquet,
DataFusion, or compression-codecs changes; also reassess before introducing
direct buffer APIs, context pooling, dictionary training, system Zstd linkage,
or additional targets. The feature check does not replace call-path review.
Upstream fixes: [zstd-rs PR 384](https://github.com/gyscos/zstd-rs/pull/384).

The XML and redirect fixes include a temporary `object_store` backport; provenance and the
removal condition are in [vendor/README.md](../src-tauri/vendor/README.md).
DataFusion 55 / Parquet 59 remove the obsolete Thrift and paste dependencies.
Keep dependency updates and these exceptions under review as advisories change.

## GitHub filtering limits

GitHub evaluates event path filters using at most 300 changed files. A matching
file beyond that list may not trigger CI; pushes exceeding 1,000 commits or
diff-generation timeouts run the workflow regardless of paths. The job filters
still select the affected checks after the workflow starts.

A workflow skipped by path filters cannot satisfy a required status check.
Before making these checks mandatory in branch protection or rulesets, replace
PR event filtering with an always-running lightweight gate that reports success
for irrelevant changes and requires every selected test job to pass.

References: [GitHub workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#onpushpull_requestpull_request_targetpathspaths-ignore),
[paths-filter](https://github.com/dorny/paths-filter).
