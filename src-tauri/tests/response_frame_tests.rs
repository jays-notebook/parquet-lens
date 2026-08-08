//! Round-trip tests for the `run_query` response frame (Plan 02-04, Task 1 — WR-03).
//!
//! `run_query` used to return only the Arrow IPC bytes, forcing the frontend to make a
//! second `get_last_result_meta` round-trip for `total_rows` / `capped` / `schema`. The
//! engine mutex is released between those two calls, so an interleaved `open_file`
//! (which clears `last_query_meta` through `register_source`) turned a successful query
//! into "No query results cached", and an interleaved `run_query` paired one execution's
//! bytes with another execution's metadata.
//!
//! The fix frames both into a single response:
//!
//!   [0..4)                 u32 little-endian meta_len
//!   [4..4+meta_len)        meta JSON (UTF-8)
//!   [4+meta_len..)         Arrow IPC stream bytes (possibly empty)
//!
//! These tests decode that layout by hand — exactly the way `decodeQueryFrame` in
//! `src/lib/tauri.ts` does — so a change to the prefix width, the endianness or the
//! segment order breaks the backend build rather than the grid at runtime.
//!
//! Tests legitimately use unwrap/expect for assertions — idiomatic in Rust test code.
#![allow(clippy::unwrap_used)]

use parquet_lens_lib::ipc::frame::{META_LEN_PREFIX_BYTES, frame_meta_and_bytes};
use parquet_lens_lib::ipc::{RunQueryResponse, SchemaField};

/// Builds a `RunQueryResponse` with two result columns, mirroring what
/// `QueryEngine::execute` caches after a real query.
fn sample_meta(total_rows: usize, capped: bool) -> RunQueryResponse {
    RunQueryResponse {
        total_rows,
        capped,
        schema: vec![
            SchemaField {
                name: "id".to_string(),
                arrow_type: "Int64".to_string(),
                nullable: false,
            },
            SchemaField {
                name: "label".to_string(),
                arrow_type: "Utf8".to_string(),
                nullable: true,
            },
        ],
    }
}

/// Splits a framed buffer the way the frontend decoder does:
/// little-endian `u32` prefix, then the JSON slice, then the remaining Arrow bytes.
fn split_frame(frame: &[u8]) -> (serde_json::Value, &[u8]) {
    assert!(
        frame.len() >= META_LEN_PREFIX_BYTES,
        "frame must carry at least the length prefix"
    );

    let mut len_bytes = [0u8; META_LEN_PREFIX_BYTES];
    len_bytes.copy_from_slice(&frame[..META_LEN_PREFIX_BYTES]);
    let meta_len = u32::from_le_bytes(len_bytes) as usize;

    let json_end = META_LEN_PREFIX_BYTES + meta_len;
    assert!(
        json_end <= frame.len(),
        "declared metadata length must fit inside the frame"
    );

    let meta: serde_json::Value =
        serde_json::from_slice(&frame[META_LEN_PREFIX_BYTES..json_end]).unwrap();

    (meta, &frame[json_end..])
}

#[test]
fn frame_round_trips_meta_and_bytes() {
    // The metadata and the row bytes must survive framing unchanged and stay paired.
    let meta = sample_meta(7, true);
    let payload: Vec<u8> = vec![0xAA, 0x01, 0x00, 0xFF, 0x42, 0x7F];

    let frame = frame_meta_and_bytes(&meta, &payload).unwrap();
    let (decoded_meta, arrow_segment) = split_frame(&frame);

    assert_eq!(decoded_meta["total_rows"], 7);
    assert_eq!(decoded_meta["capped"], true);

    let schema = decoded_meta["schema"].as_array().unwrap();
    assert_eq!(schema.len(), 2, "both result columns must survive framing");
    assert_eq!(schema[0]["name"], "id");
    assert_eq!(schema[0]["arrow_type"], "Int64");
    assert_eq!(schema[0]["nullable"], false);
    assert_eq!(schema[1]["name"], "label");
    assert_eq!(schema[1]["arrow_type"], "Utf8");
    assert_eq!(schema[1]["nullable"], true);

    assert_eq!(
        arrow_segment, payload.as_slice(),
        "the Arrow segment must be byte-for-byte identical to the input"
    );
}

#[test]
fn frame_handles_empty_arrow_payload() {
    // A zero-row result serializes to zero Arrow bytes, but its metadata is still real —
    // this is the backend half of the IN-03 fix. `capped: true` with `total_rows: 0`
    // is deliberately the combination the old frontend hardcoded away.
    let meta = sample_meta(0, true);

    let frame = frame_meta_and_bytes(&meta, &[]).unwrap();
    let (decoded_meta, arrow_segment) = split_frame(&frame);

    assert!(
        arrow_segment.is_empty(),
        "an empty payload must append nothing after the JSON"
    );

    let json_len = serde_json::to_vec(&meta).unwrap().len();
    assert_eq!(
        frame.len(),
        META_LEN_PREFIX_BYTES + json_len,
        "an empty-payload frame is exactly the prefix plus the JSON"
    );

    assert_eq!(decoded_meta["total_rows"], 0);
    assert_eq!(decoded_meta["capped"], true);
    assert_eq!(decoded_meta["schema"].as_array().unwrap().len(), 2);
}

#[test]
fn frame_prefix_is_little_endian() {
    // The frontend reads the prefix with `DataView.getUint32(0, true)`. If the backend
    // ever switched to big-endian, a 2-column result would declare a ~3 GB metadata
    // length and the decoder would reject every response.
    let meta = sample_meta(3, false);
    let json_len = serde_json::to_vec(&meta).unwrap().len();

    let frame = frame_meta_and_bytes(&meta, &[0x01, 0x02]).unwrap();

    let mut len_bytes = [0u8; META_LEN_PREFIX_BYTES];
    len_bytes.copy_from_slice(&frame[..META_LEN_PREFIX_BYTES]);

    assert_eq!(
        u32::from_le_bytes(len_bytes) as usize,
        json_len,
        "the little-endian prefix must equal the JSON byte length"
    );
    assert_eq!(
        META_LEN_PREFIX_BYTES, 4,
        "the prefix width is part of the wire contract"
    );
}
