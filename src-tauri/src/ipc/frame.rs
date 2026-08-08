//! Single-response framing for `run_query` — metadata and Arrow bytes in one payload.
//!
//! # The problem this solves (review finding WR-03)
//!
//! `run_query` used to return only the Arrow IPC bytes on the binary channel, and the
//! frontend then made a SECOND round-trip to `get_last_result_meta` for `total_rows`,
//! `capped` and the result `schema`. The engine mutex is released the moment `run_query`
//! returns, so any other command can run between the two calls:
//!
//! - An interleaved `open_file` — which is NOT gated by the frontend's `isLoading` flag,
//!   since only queries are — calls `register_source`, which clears `last_query_meta`.
//!   The follow-up `get_last_result_meta` then failed with
//!   "No query results cached. Call run_query first." for a query that had in fact
//!   succeeded and whose bytes were already in the renderer's hands.
//! - An interleaved `run_query` replaced `last_query_meta` with a different execution's
//!   metadata, so the grid silently labelled one query's rows with another query's
//!   column list and row count.
//!
//! Both are the same defect: two IPC round-trips describing state that can change in
//! between. Framing removes the window entirely — the metadata and the bytes handed to
//! the frontend are the very same tuple `QueryEngine::execute` returned, serialized
//! together while they are still provably paired.
//!
//! # Frame layout
//!
//! ```text
//! [0..4)                  u32 little-endian meta_len
//! [4..4+meta_len)         meta JSON (UTF-8) — a serialized `RunQueryResponse`
//! [4+meta_len..)          Arrow IPC stream bytes
//! ```
//!
//! The Arrow segment MAY BE EMPTY: `record_batches_to_ipc` returns an empty `Vec` when
//! the result retained zero rows. That is not an error case — the metadata segment still
//! carries the real `total_rows`, `capped` and result `schema`, which is what lets the
//! frontend render a zero-row result's column headers instead of fabricating values
//! (IN-03).
//!
//! The little-endian prefix is mirrored by `DataView.getUint32(0, true)` in
//! `src/lib/tauri.ts::decodeQueryFrame`; `response_frame_tests.rs` pins the endianness
//! so a change here breaks the build rather than the grid.
//!
//! No `.unwrap()` / `.expect()` (PITFALLS.md §Pitfall 9).

use crate::ipc::RunQueryResponse;

/// Width of the little-endian `u32` length prefix that opens every frame.
///
/// Exported so tests (and any future decoder) express the offset as a named constant
/// rather than a bare `4` — the prefix width is part of the wire contract.
pub const META_LEN_PREFIX_BYTES: usize = 4;

/// Frames `meta` and `ipc_bytes` into the single buffer returned by `run_query`.
///
/// Layout: `[u32 LE meta_len][meta JSON][ipc_bytes]`. `ipc_bytes` is appended verbatim
/// and may be empty (a zero-row result). Returns `Err(String)` if the metadata cannot be
/// serialized or is too large to describe with a `u32` length — never panics.
pub fn frame_meta_and_bytes(
    meta: &RunQueryResponse,
    ipc_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec(meta)
        .map_err(|e| format!("Failed to serialize query metadata: {}", e))?;

    // The result schema is sized by column count, so a >4 GiB metadata segment is not
    // reachable in practice — but the conversion is fallible and must not be unwrapped.
    let meta_len =
        u32::try_from(json.len()).map_err(|_| "Query metadata too large to frame".to_string())?;

    let mut frame = Vec::with_capacity(META_LEN_PREFIX_BYTES + json.len() + ipc_bytes.len());
    frame.extend_from_slice(&meta_len.to_le_bytes());
    frame.extend_from_slice(&json);
    frame.extend_from_slice(ipc_bytes);

    Ok(frame)
}
