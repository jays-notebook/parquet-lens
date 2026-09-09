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
