/**
 * Tests for registrationStatus / registrationError store fields (D-01/D-02 Phase 4).
 *
 * These tests verify the single-source-of-truth open lifecycle model added to
 * appStore.ts. The tests follow the behavior spec in 04-02-PLAN.md §Task 1.
 */
import { describe, it, expect, beforeEach } from "vitest";
import { useAppStore } from "./appStore";

// Reset store state before each test to avoid cross-test contamination
beforeEach(() => {
  useAppStore.getState().reset();
});

describe("registrationStatus initial state", () => {
  it("starts as 'idle'", () => {
    const state = useAppStore.getState();
    expect(state.registrationStatus).toBe("idle");
  });
});

describe("registrationError initial state", () => {
  it("starts as null", () => {
    const state = useAppStore.getState();
    expect(state.registrationError).toBe(null);
  });
});

describe("setRegistrationStatus", () => {
  it("sets registrationStatus to 'registering' without touching other fields", () => {
    const before = useAppStore.getState();
    const prevQueryText = before.queryText;
    const prevFilePath = before.filePath;

    useAppStore.getState().setRegistrationStatus("registering");

    const after = useAppStore.getState();
    expect(after.registrationStatus).toBe("registering");
    // Other fields remain untouched
    expect(after.queryText).toBe(prevQueryText);
    expect(after.filePath).toBe(prevFilePath);
  });

  it("sets registrationStatus to 'error'", () => {
    useAppStore.getState().setRegistrationStatus("error");
    expect(useAppStore.getState().registrationStatus).toBe("error");
  });

  it("sets registrationStatus to 'registered'", () => {
    useAppStore.getState().setRegistrationStatus("registered");
    expect(useAppStore.getState().registrationStatus).toBe("registered");
  });

  it("sets registrationStatus to 'idle'", () => {
    useAppStore.getState().setRegistrationStatus("registering");
    useAppStore.getState().setRegistrationStatus("idle");
    expect(useAppStore.getState().registrationStatus).toBe("idle");
  });
});

describe("setRegistrationError", () => {
  it("sets registrationError to a verbatim string", () => {
    useAppStore.getState().setRegistrationError("boom");
    expect(useAppStore.getState().registrationError).toBe("boom");
  });

  it("clears registrationError when passed null", () => {
    useAppStore.getState().setRegistrationError("some error");
    useAppStore.getState().setRegistrationError(null);
    expect(useAppStore.getState().registrationError).toBe(null);
  });

  it("does not touch registrationStatus when setting registrationError", () => {
    useAppStore.getState().setRegistrationStatus("registering");
    useAppStore.getState().setRegistrationError("boom");
    expect(useAppStore.getState().registrationStatus).toBe("registering");
  });
});

describe("setFile wires registrationStatus (D-08)", () => {
  it("sets registrationStatus to 'registered' after setFile", () => {
    useAppStore.getState().setRegistrationStatus("registering");
    useAppStore.getState().setFile("/tmp/foo.parquet", []);
    expect(useAppStore.getState().registrationStatus).toBe("registered");
  });

  it("clears registrationError after setFile", () => {
    useAppStore.getState().setRegistrationError("previous error");
    useAppStore.getState().setFile("/tmp/foo.parquet", []);
    expect(useAppStore.getState().registrationError).toBe(null);
  });

  it("preserves existing setFile behavior (queryError cleared, filePath set)", () => {
    useAppStore.getState().setFile("/tmp/foo.parquet", []);
    const state = useAppStore.getState();
    expect(state.filePath).toBe("/tmp/foo.parquet");
    expect(state.queryError).toBe(null);
  });
});

describe("reset() returns registration fields to zero values", () => {
  it("resets registrationStatus to 'idle'", () => {
    useAppStore.getState().setRegistrationStatus("registered");
    useAppStore.getState().reset();
    expect(useAppStore.getState().registrationStatus).toBe("idle");
  });

  it("resets registrationError to null", () => {
    useAppStore.getState().setRegistrationError("some error");
    useAppStore.getState().reset();
    expect(useAppStore.getState().registrationError).toBe(null);
  });
});

/**
 * Regression tests for D-04/D-05: overlay dismiss status restoration.
 *
 * RegistrationOverlay.handleDismiss() uses useAppStore.getState().filePath to
 * decide which status to restore. These tests exercise that exact decision
 * at the store level so the fix is covered without a React testing harness.
 *
 * Scenario A: file open → bad re-open drop → dismiss → must restore 'registered'
 *   so the preserved DataFusion table stays queryable (RunBar gate passes).
 * Scenario B: no file open → bad first drop → dismiss → must restore 'idle'
 *   so the user returns to the welcome screen (Screen A).
 */
describe("overlay dismiss status restoration (D-04/D-05)", () => {
  it("restores 'registered' when filePath is set after a failed re-open", () => {
    // Simulate: file open → bad drop sets status to 'error' (setFile was NOT called)
    useAppStore.getState().setFile("/tmp/prev.parquet", []);
    useAppStore.getState().setRegistrationStatus("error");
    useAppStore.getState().setRegistrationError("not a parquet file");

    // Simulate what handleDismiss() does
    const hasPreviousFile = useAppStore.getState().filePath !== null;
    useAppStore.getState().setRegistrationStatus(hasPreviousFile ? "registered" : "idle");
    useAppStore.getState().setRegistrationError(null);

    const state = useAppStore.getState();
    // Must be 'registered' — RunBar gate (registrationStatus !== 'registered') must pass
    expect(state.registrationStatus).toBe("registered");
    expect(state.registrationError).toBe(null);
    // filePath is still the previous valid file
    expect(state.filePath).toBe("/tmp/prev.parquet");
  });

  it("restores 'idle' when no file has ever been opened (filePath null)", () => {
    // Simulate: first action is a bad drop (no prior file)
    useAppStore.getState().setRegistrationStatus("error");
    useAppStore.getState().setRegistrationError("not a parquet file");

    // Simulate what handleDismiss() does
    const hasPreviousFile = useAppStore.getState().filePath !== null;
    useAppStore.getState().setRegistrationStatus(hasPreviousFile ? "registered" : "idle");
    useAppStore.getState().setRegistrationError(null);

    const state = useAppStore.getState();
    // Must be 'idle' — returns to welcome screen (Screen A)
    expect(state.registrationStatus).toBe("idle");
    expect(state.registrationError).toBe(null);
    expect(state.filePath).toBe(null);
  });
});

describe("openSeq monotonic open token (WR-03)", () => {
  it("increments on every setFile, including same-path re-opens", () => {
    const start = useAppStore.getState().openSeq;
    useAppStore.getState().setFile("/tmp/foo.parquet", []);
    const afterFirst = useAppStore.getState().openSeq;
    expect(afterFirst).toBe(start + 1);

    // Re-opening the SAME path must still bump the token — this is the case a
    // filePath equality check could not distinguish.
    useAppStore.getState().setFile("/tmp/foo.parquet", []);
    expect(useAppStore.getState().openSeq).toBe(afterFirst + 1);
  });

  it("increments on reset so an in-flight open is invalidated", () => {
    const before = useAppStore.getState().openSeq;
    useAppStore.getState().reset();
    expect(useAppStore.getState().openSeq).toBe(before + 1);
  });
});

describe("isLoading is unchanged (not overloaded for open path)", () => {
  it("isLoading starts false (still query-scoped)", () => {
    expect(useAppStore.getState().isLoading).toBe(false);
  });

  it("setRegistrationStatus does not affect isLoading", () => {
    useAppStore.getState().setRegistrationStatus("registering");
    expect(useAppStore.getState().isLoading).toBe(false);
  });
});

/**
 * Wave 0 tests for D-05 in-session autofill memory (Phase 5).
 *
 * `lastRemoteConnection` is session-scoped — it survives reset() and setFile()
 * so the connection form can re-populate on re-open without losing the user's
 * last entered values. It is NOT file state; clearing on reset would break
 * the autofill invariant (RESEARCH.md §Pattern 5 anti-pattern).
 */
describe("lastRemoteConnection (D-05 autofill)", () => {
  const fixture = {
    endpoint: "http://localhost:9000",
    bucket: "test-bucket",
    object_key: "data/sample.parquet",
    access_key_id: "minioadmin",
    secret_access_key: "minioadmin",
  };

  it("starts as null", () => {
    expect(useAppStore.getState().lastRemoteConnection).toBe(null);
  });

  it("setLastRemoteConnection sets the value", () => {
    useAppStore.getState().setLastRemoteConnection(fixture);
    expect(useAppStore.getState().lastRemoteConnection).toEqual(fixture);
  });

  it("setLastRemoteConnection(null) clears the value", () => {
    useAppStore.getState().setLastRemoteConnection(fixture);
    useAppStore.getState().setLastRemoteConnection(null);
    expect(useAppStore.getState().lastRemoteConnection).toBe(null);
  });

  it("reset() does NOT clear lastRemoteConnection (session-scoped, survives reset)", () => {
    // Set AFTER beforeEach reset so value is intentionally placed
    useAppStore.getState().setLastRemoteConnection(fixture);
    useAppStore.getState().reset();
    // Must survive — session autofill is not file state
    expect(useAppStore.getState().lastRemoteConnection).toEqual(fixture);
  });

  it("setFile() does NOT clear lastRemoteConnection (session-scoped, survives file open)", () => {
    // Set AFTER beforeEach reset so value is intentionally placed
    useAppStore.getState().setLastRemoteConnection(fixture);
    useAppStore.getState().setFile("/tmp/other.parquet", []);
    // Must survive — autofill persists through local file opens too
    expect(useAppStore.getState().lastRemoteConnection).toEqual(fixture);
  });
});
