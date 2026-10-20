# Remote Object Browser

Approved in conversation on 2026-10-20.

- Separate endpoint/credentials entry from bucket and object selection.
- Search the buckets returned by the server; allow manual bucket entry.
- Browse prefixes using a lazy folder tree and a file list with name, size, and modified time.
- Provide breadcrumbs, parent navigation, prefix entry, pagination, and explicit single-file opening.
- Restore the last successful connection and start in the last object's parent prefix.
- Add Browse beside Open File and Open Remote in the query toolbar, opening the same browser with the active remote file's credentials, bucket, and parent prefix. Disable it for local files and while opening or querying.
- Keep direct bucket/object entry available even when listing is denied.
- Keep browsing errors local to the dialog; distinguish loading, empty results, and failed requests.
- Discard responses from closed dialogs or superseded connections/buckets/paths.
- Support keyboard navigation, focus containment, Escape, and focus restoration.
- Reuse the existing registration pipeline and secure credential persistence. Browsing must not change the active file or saved credentials.
- Reuse object_store signing and paginated listing; do not hand-roll authentication or add a large SDK.
- Preserve exact object keys, including spaces, Unicode, percent signs, and URL delimiters.
- Preserve the existing custom endpoint, path-style addressing, internal HTTP/self-signed TLS, and us-east-1 region behavior.
- Do not add writes/deletes, multi-file opening, recursive search, or multiple saved profiles.
- UI, code comments, and documentation additions use English.

Bucket search filters loaded server results. The UI must disclose incomplete results and offer more pages. AWS ListBuckets returns owned buckets, not every accessible cross-account bucket. ListBuckets/ListObjects/GetObject permissions are independent.
