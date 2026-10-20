# Remote Object Browser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Let users discover buckets and navigate object prefixes before opening one remote Parquet file.

**Architecture:** Add read-only browser commands alongside the existing file commands, sharing storage client configuration. A connection dialog hosts an independently mounted browser with local request state; only explicit opening reaches the existing file-registration hook.

**Tech Stack:** React 18, TypeScript, Tauri 2, Rust, object_store 0.13.2, Vitest, jsdom.

**Spec:** `docs/superpowers/specs/2026-10-20-remote-browser.md`

## Global Constraints

- Reuse object_store signing and paginated listing; do not hand-roll authentication or add a large SDK.
- Preserve the existing custom endpoint, path-style addressing, internal HTTP/self-signed TLS, and us-east-1 region behavior.
- Browsing must not change the active file or saved credentials.
- Preserve exact object keys, including spaces, Unicode, percent signs, and URL delimiters.
- UI, code comments, and documentation additions use English.
- Leave the user's existing `.gitignore` edit untouched.

## Review Focus

- Listing denied while direct opening remains possible: backend sanitized errors and UI fallback tests.
- More than one page, including a page containing only folders: server request/response tests and UI pagination test.
- Responses arriving after navigation/close: deferred IPC response tests against real React components.
- Unicode, whitespace, percent/hash/question-mark keys: listing/open URL tests and exact selection test.
- Keyboard-only operation and secure restoration: DOM focus, Escape, and restoration tests.

### Task 1: Storage browsing and IPC

**Files:**
- Create: `src-tauri/src/storage/browser.rs`, `src-tauri/src/commands/remote_browser.rs`, `src-tauri/tests/remote_browser_tests.rs`.
- Modify: `src-tauri/src/storage/remote.rs`, `src-tauri/src/engine/context.rs`, `src-tauri/src/storage/mod.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`.

**Interfaces:**
- Consumes endpoint/access_key_id/secret_access_key via `RemoteCredentials`.
- Produces `list_remote_buckets(credentials, page_token) -> BucketPage { buckets, next_token }`.
- Produces `list_remote_prefix(credentials, bucket, prefix, page_token) -> PrefixPage { prefixes, objects: [{ key, size, last_modified }], next_token }`.
- Page size 200; prefix is empty for root or ends in `/`.

- [x] Write tests against a loopback HTTP server for signed bucket enumeration, encoded continuation tokens, one-level object listing, folder markers, denied/invalid responses, invalid credentials/endpoint, and exact object-key URLs.
- [x] Run targeted Rust tests and verify missing functionality fails.
- [x] Implement storage helpers, shared client options, thin Tauri commands and registration; expose existing transitive URL/XML crates as direct dependencies.
- [x] Run Rust suite; verify all tests pass.

### Task 2: Explorer and connection flow

**Files:**
- Create: `src/components/RemoteBrowser.tsx`, `src/components/RemoteConnectionModal.test.tsx`, `src/components/remote-browser.css`.
- Modify: `src/components/RemoteConnectionModal.tsx`, `src/lib/tauri.ts`, `src/lib/tauri.test.ts`, `package.json`, `package-lock.json`.

**Interfaces:**
- Typed `listRemoteBuckets`/`listRemotePrefix` wrappers mirror Task 1.
- Browser receives credentials and initial bucket/prefix/key; emits selected bucket/key for explicit opening.
- Modal continues to call `onClose()` before `onSubmit(RemoteConnection)`.

- [x] Write DOM tests for bucket filtering/manual entry, lazy navigation/breadcrumbs/pagination, selection, stale response isolation, listing failures/direct opening, saved connection restoration, and keyboard focus.
- [x] Run tests and verify failures reflect missing browsing behavior.
- [x] Implement the two-step modal, local browser state and typed IPC wrappers using existing Button/Input components.
- [x] Run frontend suite and production build.

### Task 3: Integration, documentation, and review

**Files:** `README.md`, `docs/01-tauri-and-ipc.md`, files identified by integration checks.

- [x] Update user-facing feature documentation and command table.
- [x] Run the complete frontend/Rust suites and frontend build.
- [x] Inspect the browser UI with representative fixture data where tooling permits.
- [x] Obtain a fresh whole-change review; fix material findings with regression tests.
- [x] Report implemented behavior and verification limits. Leave changes on `feat/remote-browser` for the user's normal commit/PR flow.

## Verification and implementation notes

- Frontend: 121 tests, including real React DOM behavior at the IPC boundary; production build passes.
- Rust: 76 tests, including loopback S3 HTTP responses and a real registration/query regression for unconventional object names.
- Disposable local MinIO: two buckets, 203 objects, lazy prefix navigation, two-page listing, and SQL against a key containing Unicode, spaces, `%`, `#`, `?`, and trailing whitespace passed.
- Browser preview inspected at desktop and default app dimensions; explorer height adjusted to keep the footer accessible.
- Fresh review identified filename-based format inference as incompatible with exact keys. The existing registration pipeline now explicitly chooses Parquet through DataFusion's factory, retaining session options and atomic registration. Regression test failed before the fix and passes afterward.
- Tree collapse explicitly moves a hidden descendant's focus to its ancestor; DOM regression failed before the fix and passes afterward.
- Listing HTTP errors in object_store 0.13 use a private wrapper. A narrowly matched status prefix is classified into sanitized messages; integration tests pin this dependency boundary.
- The query toolbar's Browse shortcut reuses the active remote file's credentials and parent prefix. Active connection state survives failed opens and is cleared by local opens/reset; saved autofill credentials remain separate. App integration tests cover direct browsing, file selection, cancellation, reopening, and disabled states.
- The user verified the macOS app and requested committing, pushing, and opening a PR against main.
- Production AWS/MinIO policies and Windows native UI were not exercised.
