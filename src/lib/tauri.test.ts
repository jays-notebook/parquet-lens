/**
 * Behavioral tests for openRemoteFile() invoke wrapper (REMOTE-01)
 * and for the result-schema-driven row decoding helpers (Phase 1, Plan 01).
 *
 * Verifies that openRemoteFile() invokes the Tauri command named exactly
 * "open_remote_file" with payload shape `{ conn }` containing all 5
 * snake_case fields of RemoteConnection.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock @tauri-apps/api/core before importing the module under test.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { tableFromArrays, tableToIPC } from "apache-arrow";
import { invoke } from "@tauri-apps/api/core";
import {
  arrowTableToRows,
  decodeQueryFrame,
  dedupeFieldKeys,
  displayColumnName,
  openRemoteFile,
  runQuery,
  type RemoteConnection,
  type OpenFileResponse,
  type SchemaField,
} from "./tauri";

const mockInvoke = vi.mocked(invoke);

const FIXTURE: RemoteConnection = {
  endpoint: "http://localhost:9000",
  bucket: "test-bucket",
  object_key: "data/sample.parquet",
  access_key_id: "minioadmin",
  secret_access_key: "minioadmin",
};

const MINIMAL_RESPONSE: OpenFileResponse = {
  schema: [],
};

beforeEach(() => {
  mockInvoke.mockReset();
});

describe("openRemoteFile IPC contract (REMOTE-01)", () => {
  it("invokes 'open_remote_file' exactly once with { conn } payload", async () => {
    mockInvoke.mockResolvedValueOnce(MINIMAL_RESPONSE);

    await openRemoteFile(FIXTURE);

    expect(mockInvoke).toHaveBeenCalledTimes(1);
    expect(mockInvoke).toHaveBeenCalledWith("open_remote_file", { conn: FIXTURE });
  });

  it("passes all 5 snake_case RemoteConnection fields verbatim", async () => {
    mockInvoke.mockResolvedValueOnce(MINIMAL_RESPONSE);

    await openRemoteFile(FIXTURE);

    const [command, payload] = mockInvoke.mock.calls[0] as [string, { conn: RemoteConnection }];
    expect(command).toBe("open_remote_file");
    expect(payload.conn.endpoint).toBe(FIXTURE.endpoint);
    expect(payload.conn.bucket).toBe(FIXTURE.bucket);
    expect(payload.conn.object_key).toBe(FIXTURE.object_key);
    expect(payload.conn.access_key_id).toBe(FIXTURE.access_key_id);
    expect(payload.conn.secret_access_key).toBe(FIXTURE.secret_access_key);
  });

  it("returns the OpenFileResponse from invoke", async () => {
    const response: OpenFileResponse = {
      schema: [{ name: "id", arrow_type: "Int64", nullable: false }],
    };
    mockInvoke.mockResolvedValueOnce(response);

    const result = await openRemoteFile(FIXTURE);

    expect(result).toEqual(response);
  });
});

describe("dedupeFieldKeys", () => {
  it("leaves a duplicate-free list unchanged", () => {
    expect(dedupeFieldKeys(["id", "name"])).toEqual(["id", "name"]);
  });

  it("suffixes the second occurrence with __2", () => {
    expect(dedupeFieldKeys(["price", "price"])).toEqual(["price", "price__2"]);
  });

  it("numbers each further occurrence in order", () => {
    expect(dedupeFieldKeys(["price", "price", "price"])).toEqual([
      "price",
      "price__2",
      "price__3",
    ]);
  });

  it("counts occurrences per name when duplicates interleave", () => {
    expect(dedupeFieldKeys(["a", "b", "a", "b", "a"])).toEqual([
      "a",
      "b",
      "a__2",
      "b__2",
      "a__3",
    ]);
  });

  it("returns an empty array for an empty input", () => {
    expect(dedupeFieldKeys([])).toEqual([]);
  });

  it("probes past an existing __N name", () => {
    // `select price, x as price__2, price from data` — the generated key for the
    // third column would collide with the literal second column name (WR-01).
    expect(dedupeFieldKeys(["price", "price__2", "price"])).toEqual([
      "price",
      "price__2",
      "price__3",
    ]);
  });

  it("pushes a literal __N source name forward", () => {
    // The generated `a__2` claims the key first, so the literal source name
    // `a__2` must advance rather than collide (WR-01).
    expect(dedupeFieldKeys(["a", "a", "a__2"])).toEqual(["a", "a__2", "a__2__2"]);
  });

  it("always preserves length and produces unique keys", () => {
    const input = ["x", "x", "x__2", "x", "x__2", "y"];
    const out = dedupeFieldKeys(input);

    expect(out).toHaveLength(input.length);
    expect(new Set(out).size).toBe(input.length);
  });
});

describe("displayColumnName", () => {
  it("cleans the DataFusion count(*) output name to count", () => {
    expect(displayColumnName("count(*)")).toBe("count");
  });

  it("matches the count(*) output name case-insensitively", () => {
    expect(displayColumnName("COUNT(*)")).toBe("count");
    expect(displayColumnName("Count(*)")).toBe("count");
  });

  it("tolerates surrounding whitespace around count(*)", () => {
    expect(displayColumnName("count(*) ")).toBe("count");
    expect(displayColumnName("  count(*)  ")).toBe("count");
  });

  it("never rewrites an explicit SQL alias", () => {
    // `select count(*) as n` — DataFusion already named the column `n`, so it
    // no longer matches the star form and must pass through untouched.
    expect(displayColumnName("n")).toBe("n");
    expect(displayColumnName("total")).toBe("total");
  });

  it("only cleans the star form, not count over a column", () => {
    expect(displayColumnName("count(price)")).toBe("count(price)");
    expect(displayColumnName("count(1)")).toBe("count(1)");
  });

  it("passes every other aggregate and plain name through verbatim", () => {
    expect(displayColumnName("sum(price)")).toBe("sum(price)");
    expect(displayColumnName("avg(price)")).toBe("avg(price)");
    expect(displayColumnName("id")).toBe("id");
  });

  it("preserves original whitespace and casing of passthrough names", () => {
    expect(displayColumnName("  Sum(Price) ")).toBe("  Sum(Price) ");
  });

  it("returns an empty string unchanged", () => {
    expect(displayColumnName("")).toBe("");
  });
});

describe("arrowTableToRows", () => {
  it("reads columns positionally and converts bigint cells to strings", () => {
    const table = tableFromArrays({
      id: BigInt64Array.from([1n, 2n]),
      ratio: Float64Array.from([1.5, 2.5]),
    });

    expect(arrowTableToRows(table)).toEqual([
      { id: "1", ratio: 1.5 },
      { id: "2", ratio: 2.5 },
    ]);
  });
});

/**
 * Assembles the same frame `src-tauri/src/ipc/frame.rs::frame_meta_and_bytes` produces:
 * `[u32 LE meta_len][meta JSON][arrow IPC bytes]`.
 *
 * Written by hand rather than shared with the decoder so a change to the layout has to
 * be made in two places to pass — a decoder-only regression cannot self-approve.
 */
function buildFrame(meta: unknown, arrowBytes: Uint8Array): ArrayBuffer {
  const json = new TextEncoder().encode(JSON.stringify(meta));
  const frame = new ArrayBuffer(4 + json.byteLength + arrowBytes.byteLength);

  new DataView(frame).setUint32(0, json.byteLength, true);
  const out = new Uint8Array(frame);
  out.set(json, 4);
  out.set(arrowBytes, 4 + json.byteLength);

  return frame;
}

const TWO_COLUMN_SCHEMA: SchemaField[] = [
  { name: "id", arrow_type: "Int64", nullable: false },
  { name: "ratio", arrow_type: "Float64", nullable: true },
];

describe("decodeQueryFrame", () => {
  it("decodes rows and metadata from a single framed response", () => {
    const arrow = tableToIPC(
      tableFromArrays({
        id: BigInt64Array.from([1n, 2n]),
        ratio: Float64Array.from([1.5, 2.5]),
      })
    );
    const frame = buildFrame(
      { total_rows: 2, capped: false, schema: TWO_COLUMN_SCHEMA },
      arrow
    );

    const result = decodeQueryFrame(frame);

    expect(result.rows).toEqual([
      { id: "1", ratio: 1.5 },
      { id: "2", ratio: 2.5 },
    ]);
    expect(result.total_rows).toBe(2);
    expect(result.capped).toBe(false);
    expect(result.schema).toEqual(TWO_COLUMN_SCHEMA);
  });

  it("takes total_rows and capped from the frame metadata when the arrow body is empty", () => {
    // IN-03: the zero-byte branch used to hardcode `total_rows: 0, capped: false`.
    // `capped: true` is the combination that makes a regression to those literals fail.
    const schema: SchemaField[] = [{ name: "id", arrow_type: "Int64", nullable: false }];
    const frame = buildFrame(
      { total_rows: 0, capped: true, schema },
      new Uint8Array(0)
    );

    const result = decodeQueryFrame(frame);

    expect(result.rows).toEqual([]);
    expect(result.total_rows).toBe(0);
    expect(result.capped).toBe(true);
    expect(result.schema).toEqual(schema);
  });

  it("throws on a frame shorter than the 4-byte length prefix", () => {
    expect(() => decodeQueryFrame(new ArrayBuffer(2))).toThrow(
      /Malformed query response/
    );
  });

  it("throws when the declared metadata length exceeds the response size", () => {
    // A well-formed prefix claiming more JSON than the buffer holds — a truncated
    // response must fail explicitly rather than slice out of bounds (T-02-14).
    const frame = new ArrayBuffer(12);
    new DataView(frame).setUint32(0, 9999, true);

    expect(() => decodeQueryFrame(frame)).toThrow(/Malformed query response/);
  });
});

describe("runQuery IPC contract (WR-03)", () => {
  it("performs exactly one invoke, with 'run_query' and { sql }", async () => {
    const frame = buildFrame(
      { total_rows: 0, capped: false, schema: [] },
      new Uint8Array(0)
    );
    mockInvoke.mockResolvedValueOnce(frame);

    await runQuery("select 1");

    // Two round-trips (run_query + get_last_result_meta) is exactly the
    // desynchronisation window WR-03 describes; one call proves it is closed.
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    expect(mockInvoke).toHaveBeenCalledWith("run_query", { sql: "select 1" });
  });
});
