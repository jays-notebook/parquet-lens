/**
 * Typed invoke wrappers for Tauri IPC commands.
 *
 * This file is the single source of truth for frontend types. Components import
 * from here — not from `@tauri-apps/api` directly (ARCHITECTURE.md §IPC Command Surface).
 *
 * TypeScript interface field names mirror Rust `serde` output (snake_case keys, since
 * Tauri serializes Rust structs as-is without renaming).
 *
 * # IPC design for run_query (CONTEXT.md D-06, STACK.md §IPC Serialization Strategy)
 *
 * Row data travels as Arrow IPC binary (ArrayBuffer) via `run_query` →
 * `tauri::ipc::Response`. Metadata (total_rows, capped) travels as JSON via
 * `get_last_result_meta`. The two-call pattern keeps bulk row data off the JSON channel.
 */
import { invoke } from "@tauri-apps/api/core";
import { tableFromIPC, type Table } from "apache-arrow";

/** A single column in the registered `data` table's Arrow schema. */
export interface SchemaField {
  name: string;
  /** Arrow-native type name, e.g. "Int64", "Utf8", "Float64". */
  arrow_type: string;
  nullable: boolean;
}

/** Returned by `open_file` after the file is registered as table `data`. */
export interface OpenFileResponse {
  schema: SchemaField[];
}

/** Returned by `get_last_result_meta` after SQL execution. */
export interface RunQueryResponse {
  total_rows: number;
  /** `true` when the backend 100-row cap was hit. */
  capped: boolean;
}

/** The decoded query result ready for rendering. */
export interface QueryResult {
  /** Arrow Table object from tableFromIPC — column-oriented. */
  table: Table;
  /** Row objects converted from the Arrow Table for TanStack Table consumption. */
  rows: Record<string, unknown>[];
  /** Total rows in the result (at most 100). */
  total_rows: number;
  /** True when the 100-row backend cap was hit. */
  capped: boolean;
}

/** A single page of result rows returned by `get_page`. */
export interface PageResponse {
  rows: Record<string, unknown>[];
  offset: number;
  has_more: boolean;
}

/**
 * Opens a Parquet file at `path` and registers it as SQL table `data`.
 * Returns the inferred Arrow schema on success.
 */
export async function openFile(path: string): Promise<OpenFileResponse> {
  return invoke<OpenFileResponse>("open_file", { path });
}

/**
 * Connection parameters for a remote MinIO/S3 Parquet object.
 *
 * Field names are snake_case throughout — they must match the Rust serde field names
 * in `RemoteConnection` on the backend (Tauri serializes without renaming).
 * See: src-tauri/src/commands/file.rs `RemoteConnection` struct.
 */
export interface RemoteConnection {
  endpoint: string;
  bucket: string;
  /** Path to the Parquet object within the bucket, e.g. "prefix/file.parquet". */
  object_key: string;
  access_key_id: string;
  secret_access_key: string;
}

/**
 * Opens a remote Parquet object at the given S3/MinIO endpoint
 * and registers it as SQL table `data`.
 *
 * Tauri serializes `{ conn }` as `{ "conn": { ... } }` matching the Rust
 * command parameter `conn: RemoteConnection` (Phase 5, REMOTE-01).
 */
export async function openRemoteFile(
  conn: RemoteConnection
): Promise<OpenFileResponse> {
  return invoke<OpenFileResponse>("open_remote_file", { conn });
}

/**
 * Executes `sql` against the currently registered `data` table.
 *
 * Uses the two-command pattern (STACK.md §IPC Serialization Strategy):
 *   1. `run_query` → Arrow IPC ArrayBuffer (row data on binary channel)
 *   2. `get_last_result_meta` → JSON (total_rows, capped metadata)
 *
 * Decodes the Arrow IPC bytes with `tableFromIPC` from `apache-arrow`.
 * Converts the columnar Arrow Table into row objects for TanStack Table.
 */
export async function runQuery(sql: string): Promise<QueryResult> {
  // Step 1: Get Arrow IPC bytes (binary channel).
  const ipcBuffer = await invoke<ArrayBuffer>("run_query", { sql });

  // Step 2: Get metadata (JSON channel — lightweight, no row data).
  const meta = await invoke<RunQueryResponse>("get_last_result_meta");

  if (!ipcBuffer || ipcBuffer.byteLength === 0) {
    return {
      table: tableFromIPC(new Uint8Array(0)),
      rows: [],
      total_rows: 0,
      capped: false,
    };
  }

  // Decode Arrow IPC bytes into a columnar Table.
  const table = tableFromIPC(ipcBuffer);

  // Convert the columnar Arrow Table to row objects for TanStack Table consumption.
  // Each row is `Record<string, unknown>` with column names as keys.
  const rows = arrowTableToRows(table);

  return {
    table,
    rows,
    total_rows: meta.total_rows,
    capped: meta.capped,
  };
}

/**
 * Converts a columnar Apache Arrow `Table` to an array of row objects.
 *
 * Each cell value is converted to a display-safe representation:
 * - null → undefined (renders as blank in the grid, D-06)
 * - bigint → string (avoids JSON.stringify issues)
 * - all other primitives → kept as-is
 */
export function arrowTableToRows(table: Table): Record<string, unknown>[] {
  const rows: Record<string, unknown>[] = [];
  const numRows = table.numRows;
  const schema = table.schema;

  for (let r = 0; r < numRows; r++) {
    const row: Record<string, unknown> = {};
    for (const field of schema.fields) {
      const col = table.getChild(field.name);
      if (!col) {
        row[field.name] = undefined;
        continue;
      }
      const raw = col.get(r);
      if (raw === null || raw === undefined) {
        // NULL → undefined so the grid renders a blank cell (D-06).
        row[field.name] = undefined;
      } else if (typeof raw === "bigint") {
        // BigInt cannot be passed directly to React as a cell value.
        row[field.name] = raw.toString();
      } else {
        row[field.name] = raw;
      }
    }
    rows.push(row);
  }

  return rows;
}

/** Fetches a page of query results from the backend result cache. */
export async function getPage(
  offset: number,
  size: number
): Promise<PageResponse> {
  return invoke<PageResponse>("get_page", { offset, size });
}

/** Per-row-group statistics from the Parquet footer (META-03). */
export interface RowGroupInfo {
  num_rows: number;
  total_byte_size: number;
  /** Compression codec name, e.g. "SNAPPY", "ZSTD", "UNCOMPRESSED". */
  compression: string;
}

/** File-level Parquet footer metadata (META-02 + META-03). */
export interface FileMetadata {
  /** Total row count summed from all row groups (META-02). */
  total_rows: number;
  row_groups: RowGroupInfo[];
}

/**
 * Fetches Parquet footer metadata for the currently registered file.
 * Call after openFile succeeds — returns total row count and per-row-group stats.
 */
export async function getFileMetadata(): Promise<FileMetadata> {
  return invoke<FileMetadata>("get_file_metadata");
}
