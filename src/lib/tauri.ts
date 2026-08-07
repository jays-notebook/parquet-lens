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
 * `tauri::ipc::Response`. Metadata (total_rows, capped, and the query RESULT schema)
 * travels as JSON via `get_last_result_meta`. The two-call pattern keeps bulk row data
 * off the JSON channel.
 *
 * The result schema belongs on the metadata channel because it is sized by column
 * count, not row count — it stays far below the JSON weight budget while bulk row
 * data remains on the binary channel. Sourcing it from the backend (rather than
 * deriving type labels from the decoded Arrow-JS table) is what makes the grid's
 * type labels identical to the sidebar's file-schema labels: both are produced by
 * the same Rust `format!("{:?}", data_type)` expression (D-PH1-01).
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
  /**
   * Schema of the QUERY RESULT — not the opened file's schema.
   *
   * Supplied by the backend on the JSON metadata channel. Its `arrow_type`
   * strings are produced by the same Rust expression as the file schema, so the
   * two are identical by construction for every Arrow type (D-PH1-01).
   */
  schema: SchemaField[];
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
  /**
   * Schema of the QUERY RESULT, straight from the backend metadata channel —
   * never derived from the decoded Arrow-JS table. This is the sole source of
   * the grid's columns and typed headers (D-PH1-01).
   */
  schema: SchemaField[];
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
      // A zero-row result still knows its columns — the backend takes the schema
      // from the executed stream, not from the retained batches.
      schema: meta.schema,
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
    schema: meta.schema,
  };
}

/**
 * Makes a positional list of column names unique, preserving order and length.
 *
 * The nth occurrence (n ≥ 2) of a name becomes `${name}__${n}`; the first
 * occurrence keeps the bare name.
 *
 * # Why this exists (D-PH1-03)
 *
 * `arrowTableToRows` returns `Record<string, unknown>`, so two result columns
 * with the same name — which SQL happily produces, e.g.
 * `select a, b as a from data` — would collapse to a single object key and the
 * grid would render duplicate TanStack column ids. Deduping positionally keeps
 * each column independent, with its own values.
 *
 * The ORIGINAL name is still what the user sees: the grid header renders
 * `SchemaField.name`, while the deduped key is only ever a lookup key.
 */
export function dedupeFieldKeys(names: string[]): string[] {
  const seen = new Map<string, number>();

  return names.map((name) => {
    const count = (seen.get(name) ?? 0) + 1;
    seen.set(name, count);
    return count === 1 ? name : `${name}__${count}`;
  });
}

/**
 * The literal output-column name DataFusion 54 emits for `select count(*)`.
 * Compared against the lowercased, trimmed input — never used as a row key.
 */
const COUNT_STAR_OUTPUT_NAME = "count(*)";

/**
 * Maps a result-column name to the name shown to the user.
 *
 * DataFusion 54 names the output column of `select count(*) from data`
 * literally `count(*)`. PROJECT.md locks the display form as a clean `count`,
 * so the user reads a plain `count : <value>` presentation. Every other name —
 * including other aggregates (`sum(price)`, `avg(price)`), computed columns and
 * explicit SQL aliases — passes through verbatim, with its original whitespace
 * and casing intact. An alias always wins: `select count(*) as n` makes
 * DataFusion name the column `n`, which no longer matches and is never rewritten.
 *
 * # PRESENTATION ONLY — never use this as a key
 *
 * Column ids, `accessorFn` lookups and row-object keys all use the deduped keys
 * from {@link dedupeFieldKeys}, which are derived from the RAW backend names.
 * Feeding this function's result into an id, an accessor, or `dedupeFieldKeys`
 * would desynchronise column ids from row keys and make every cell resolve to
 * `undefined` (rendering as NULL). Use it only where a name is displayed:
 * header text and the `<th>` tooltip.
 */
export function displayColumnName(name: string): string {
  if (name.trim().toLowerCase() === COUNT_STAR_OUTPUT_NAME) {
    return "count";
  }
  // Passthrough is the ORIGINAL argument, untrimmed — the trim above is only a
  // comparison tolerance, not a normalisation applied to every name.
  return name;
}

/**
 * Converts a columnar Apache Arrow `Table` to an array of row objects.
 *
 * Columns are read POSITIONALLY via `getChildAt(i)` rather than by name, so two
 * same-named result columns produce two independent values instead of the same
 * column being read twice (D-PH1-03). Each value is keyed by the deduped key
 * for its position.
 *
 * Each cell value is converted to a display-safe representation:
 * - null → undefined (renders as blank in the grid, D-06)
 * - bigint → string (avoids JSON.stringify issues)
 * - all other primitives → kept as-is
 */
export function arrowTableToRows(table: Table): Record<string, unknown>[] {
  const rows: Record<string, unknown>[] = [];
  const numRows = table.numRows;
  const fields = table.schema.fields;
  // Computed once: the same positional field-name list the grid derives its
  // column ids from, so row keys and column ids always agree.
  const keys = dedupeFieldKeys(fields.map((f) => f.name));

  for (let r = 0; r < numRows; r++) {
    const row: Record<string, unknown> = {};
    for (let i = 0; i < fields.length; i++) {
      const key = keys[i];
      const col = table.getChildAt(i);
      if (!col) {
        row[key] = undefined;
        continue;
      }
      const raw = col.get(r);
      if (raw === null || raw === undefined) {
        // NULL → undefined so the grid renders a blank cell (D-06).
        row[key] = undefined;
      } else if (typeof raw === "bigint") {
        // BigInt cannot be passed directly to React as a cell value.
        row[key] = raw.toString();
      } else {
        row[key] = raw;
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
