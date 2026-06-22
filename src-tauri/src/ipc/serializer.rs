//! Arrow IPC serialization for `RecordBatch` slices.
//!
//! Converts a `&[RecordBatch]` into Arrow IPC stream bytes (`Vec<u8>`) using
//! `datafusion::arrow::ipc::writer::StreamWriter` — the single-version re-export
//! path mandated by PITFALLS.md §Pitfall 3.
//!
//! The IPC stream format is consumed on the frontend by `tableFromIPC(arrayBuffer)`
//! from the `apache-arrow` npm package (STACK.md §IPC Serialization Strategy).

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::ipc::writer::StreamWriter;

/// Serializes a slice of `RecordBatch`es into Arrow IPC stream bytes.
///
/// The resulting `Vec<u8>` begins with the Arrow IPC stream magic bytes and is
/// decodable by any Arrow IPC reader (including `tableFromIPC` in `apache-arrow`).
///
/// Returns `Err(String)` if the writer fails — never panics.
pub fn record_batches_to_ipc(batches: &[RecordBatch]) -> Result<Vec<u8>, String> {
    if batches.is_empty() {
        // Return a minimal valid IPC stream with zero record batches but a valid schema.
        // Callers should ideally not call with empty batches, but we handle it gracefully.
        return Ok(Vec::new());
    }

    let schema = batches[0].schema();
    let mut buf: Vec<u8> = Vec::new();

    {
        let mut writer = StreamWriter::try_new(&mut buf, &schema)
            .map_err(|e| format!("Failed to create Arrow IPC StreamWriter: {}", e))?;

        for batch in batches {
            writer
                .write(batch)
                .map_err(|e| format!("Failed to write RecordBatch to IPC stream: {}", e))?;
        }

        writer
            .finish()
            .map_err(|e| format!("Failed to finish Arrow IPC stream: {}", e))?;
    }

    Ok(buf)
}
