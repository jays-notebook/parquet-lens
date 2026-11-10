# Security Patch 1.1.2 Release Plan

> **For agentic workers:** Use superpowers:executing-plans to complete this release.

**Goal:** Publish version 1.1.2 with the fix for RUSTSEC-2026-0442.

**Architecture:** Keep the existing application and tagged installer workflow. Update only the affected Arrow dependencies and application version, merge through a checked pull request, and publish the completed draft release.

**Tech Stack:** Tauri, Rust, React, GitHub Actions, OSV-Scanner.

**Spec:** The user requested that the security fix be versioned and publicly released.

## Constraints

- Preserve the user's unrelated `.gitignore` change.
- Retain the reviewed OSV exceptions without adding new exclusions.
- Use version 1.1.2 in both npm files, both Cargo files, and `tauri.conf.json`.
- Require passing CI and installer builds for Windows and universal macOS before publication.

## Release checklist

- [x] Synchronize version metadata and verify only the intended dependencies changed.
- [x] Run frontend tests/build, Rust tests, OSV scanning, and the release feature guard.
- [ ] Review, commit, push, and merge the pull request after checks pass.
- [ ] Tag the merged commit as `v1.1.2` and wait for both installer jobs.
- [ ] Verify release assets, add security release notes, publish as latest, and verify public availability.
