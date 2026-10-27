# Security Hardening Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task by task.

**Goal:** Remediate the security audit approved in the conversation on 2026-10-26.

**Architecture:** Keep the existing Tauri, DataFusion, and React boundaries. Validate every remote endpoint in the shared storage layer, require verified HTTPS except for loopback HTTP tunnels, update vulnerable dependencies, and add automated advisory checks.

**Tech Stack:** Rust/Tauri/DataFusion, React/TypeScript/Vite, GitHub Actions.

**Spec:** The preceding security audit and the user's instruction to proceed with all findings.

## Constraints and decisions

- Preserve the user's existing `.gitignore` edits and local data.
- Work on `security/2026-10-hardening`; leave changes reviewable without publishing.
- Require verified HTTPS for non-loopback endpoints. Allow HTTP only for `localhost`, IPv4 loopback, and IPv6 loopback, preserving SSH tunnel access.
- Use the OS trust store for private CAs. Do not add a TLS-verification bypass or alter the user's trust store.
- Fix transitive vulnerable copies too. Do not hide active advisories with blanket ignores.
- Update local Node.js to at least 24.22.1 and rustup to at least 1.29.2 using their existing managers.
- Do not send the dependency inventory to npm audit; use public advisory data locally.

## Review focus

- Direct file opens and bucket/prefix browsing must enforce the same endpoint policy.
- Untrusted TLS certificates must fail before an authenticated HTTP request is sent.
- Loopback HTTP and object keys with special characters must continue to work.
- Rust version changes must not leave an old XML parser in the runtime graph.
- CI must check current advisories on a schedule even when source files do not change.

## Tasks

1. **Remote transport:** Add failing endpoint-policy and untrusted-certificate tests. Centralize endpoint validation and secure client options. Update UI guidance and README. Verify the targeted tests and full Rust suite.
2. **Rust dependencies:** Resolve patched TLS, XML, HTTP, Tokio, and anyhow dependencies, including transitive consumers. Check compiler/API compatibility, run the full Rust suite, and compare the lockfile with the downloaded OSV data.
3. **JavaScript dependencies:** Upgrade supported Vite/Vitest and vulnerable transitive build dependencies. Run frontend tests/build and compare the lockfile with OSV data.
4. **Development tools and CI:** Update the existing mise Node installation and rustup, configure reproducible supported tool versions, add a scheduled/PR advisory workflow, and document its operation.
5. **Final verification:** Run all tests/builds and advisory checks; obtain an independent review of the full diff, address significant findings, and record remaining platform limits.

## Progress

- Baseline: frontend 121 tests and Rust 76 tests passed during the audit.
- Preparation: created the security branch; only pre-existing change was `.gitignore`.
- Remote transport: the two new policy/TLS tests failed against the original code, then passed after the shared validation and certificate verification fix.
- Rust dependencies: patched TLS, HTTP, Tokio, anyhow, and XML dependencies. The object_store 0.13.2 backport changes its quick-xml requirement and disables native client redirects; all other source is byte-identical to the published crate.
- JavaScript: clean `npm ci`, all 121 tests, and the production frontend build pass with Vite 8.3.5 and Vitest 5.0.5.
- Local tools: Node.js 24.22.1 and rustup 1.29.2 installed and version-checked.
- Advisory check: OSV-Scanner 2.7.0 checks 190 npm and 609 Rust packages, including the vendored object_store. It passes with only the two documented Linux-only advisory exclusions.
- Before review: all 79 Rust tests and the production configuration compile passed.
- Independent review: one Important finding (loopback HTTP redirects bypass endpoint validation), no other Critical/Important/Minor findings. Reviewer independently ran the security/browser tests and the offline OSV check.
- Final: fixed redirect policy bypass — `redirects_cannot_bypass_endpoint_validation` RED→GREEN, covering bucket/prefix/file requests; full Rust suite 80/80 passed, production configuration `cargo check --locked --features custom-protocol` passed.
- Final checks: frontend 121/121, clean npm install and production build, YAML parsing, Rust formatting, vendor diff/provenance, and `git diff --check` passed. The installer script retains UTF-8 BOM for Korean output in Windows PowerShell 5.1; Windows execution remains unverified locally.
- Final advisory result: 190 npm + 609 Rust packages scanned; no findings outside the two expiring Linux-only exclusions. No dependency inventory was uploaded. No minor review findings were deferred.
- Complete: changes remain uncommitted on `security/2026-10-hardening`; no push, merge, release, trust-store change, or production-service mutation was performed.

## Implementation decisions

- Ruling: upgrade DataFusion to 55.2 / Parquet 59.3.1 to remove the Thrift advisory and unmaintained paste dependency. The removed ListingOptions helper is no longer necessary because partition/statistics settings are read from the scan-time session. Cost if incompatible: SQL or Parquet behavior regression; the existing fixture/query suite is the gate.
- Ruling: retain only the glib/proc-macro-error Linux exclusions, expiring 2027-01-24. Both are absent from supported Windows/macOS dependency trees. Cost if Linux is introduced without revisiting them: affected GTK code could ship.
- Ruling: use a minimal object_store backport until DataFusion supports patched upstream XML and equivalent redirect control. Cost: maintain and remove the vendor override on a future compatible update. The final review extends this from a manifest-only patch to one native client builder setting.
- Ruling: retain changes on the security branch without committing or publishing; preserve the user's unrelated `.gitignore` edit. This keeps the complete result reviewable without external side effects.
- Final: Ruling: reject automatic redirects, including HTTPS redirects, across storage clients. Users must configure the final API endpoint; this prevents unchecked destinations without duplicating the transport client.
- Final: Ruling: Windows/macOS Intel installers and the real internal CA/storage are outside local verification. Current macOS tests and production configuration compilation pass; platform CI and real deployment validation remain necessary.

## Release follow-up: 2026-10-28

- The user requested a commit, PR, and version bump for a new release. This supersedes the initial decision to leave the work uncommitted.
- Synchronized version 1.1.1 across package.json, package-lock.json, Cargo.toml, Cargo.lock, and tauri.conf.json. The existing v-tag installer workflow remains the release mechanism.
- Fresh verification exposed a macOS test-server race: accepted sockets inherited nonblocking mode. Explicitly restoring blocking mode fixed the observed WouldBlock failure; targeted security tests and the full 80-test Rust suite now pass.
- Frontend 121/121 tests, production build, and Rust custom-protocol compilation passed again for version 1.1.1.
- The refreshed public advisory databases added 13 Zstd findings. Independent source and feature review found their triggers unreachable in the locked Windows/macOS build graphs. These are applicability exceptions, not package fixes, and expire on 2026-11-27. Exact reasons and review triggers are in osv-scanner.toml and docs/ci.md.
- Added a backend CI guard against enabling experimental, seekable, legacy, zstdmt, or zdict_builder features. Verified the actual graph passes and each prohibited feature is rejected by the guard.
- The final offline scan of 190 npm and 609 Rust packages passes after 15 explicitly documented, expiring exceptions (2 Linux-only and 13 Zstd). No dependency inventory was uploaded.
- Preserve the user's pre-existing .gitignore edit outside the release commit. Actual merge/tag/publication timing is separate from preparing the PR.
