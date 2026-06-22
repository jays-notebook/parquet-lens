/**
 * Behavioral tests for openRemoteFile() invoke wrapper (REMOTE-01).
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

import { invoke } from "@tauri-apps/api/core";
import { openRemoteFile, type RemoteConnection, type OpenFileResponse } from "./tauri";

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
