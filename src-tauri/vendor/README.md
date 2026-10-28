# object_store security backport

`object_store/` contains the source, manifest, and license files from the published
Apache `object_store` 0.13.2 crate, with two narrowly scoped changes:

- `Cargo.toml`: the `quick-xml` requirement changes from `0.39.0` to `0.41.0`.
- `src/client/mod.rs`: the native reqwest client disables automatic redirects.
  Redirects would otherwise let a loopback HTTP endpoint send requests to
  unchecked external HTTP destinations. All storage clients share this setting.

Published crate SHA-256: `622acbc9100d3c10e2ee15804b0caa40e55c933d5aa53814cd520805b7958a49`.
All other source files are byte-identical to that release.

This removes RUSTSEC-2026-0194 and RUSTSEC-2026-0195 from S3 response parsing.
Upstream made the same dependency upgrade for object_store 0.14, but DataFusion
54 and 55 require object_store 0.13, whose latest release is 0.13.2.
Cargo's path patch applies the backport to every consumer, including DataFusion.

Upstream references:
- https://crates.io/crates/object_store/0.13.2
- https://github.com/apache/arrow-rs-object-store/pull/785
- https://rustsec.org/advisories/RUSTSEC-2026-0194.html
- https://rustsec.org/advisories/RUSTSEC-2026-0195.html

Remove this directory and `[patch.crates-io]` once a compatible upstream release
uses quick-xml 0.41 or later and provides equivalent redirect control. Preserve
the application's no-redirect policy when migrating; the regression test covers
bucket listing, prefix listing, and file reads. Keep any further changes explicit.
