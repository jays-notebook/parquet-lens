/**
 * Application-wide Zustand store.
 *
 * `filePath === null` means the welcome screen (Screen A) is shown.
 * Once a file is opened, `filePath` is set and Screen B is rendered.
 *
 * Source: ARCHITECTURE.md §App State (Zustand)
 */
import { create } from "zustand";
import type { FileMetadata, RemoteConnection, SchemaField } from "../lib/tauri";

/**
 * D-01 (Phase 4): Single source of truth for the file open/registration lifecycle.
 * Drives the blocking overlay, query gating, and error display.
 * `isLoading` remains QUERY-scoped — do not overload it for the open path.
 */
export type RegistrationStatus = "idle" | "registering" | "registered" | "error";

/** Default SQL query shown in the editor after a file is opened (CONTEXT.md locked). */
const DEFAULT_QUERY = "select * from data limit 100";

interface AppState {
  // File state
  filePath: string | null;
  schema: SchemaField[];

  // Query state
  queryText: string;
  isLoading: boolean;
  /**
   * Inline query error shown below the SQL editor (D-01 / QUERY-04).
   * Set by run-handlers on catch; cleared on edit, file open, and reset (D-04).
   */
  queryError: string | null;

  // Result state
  rows: Record<string, unknown>[];
  totalRows: number;
  capped: boolean;

  // Sidebar state (Phase 3)
  /** Whether the schema/metadata left sidebar is collapsed. Default: false (open). */
  sidebarCollapsed: boolean;
  /** File-level metadata returned by get_file_metadata command. Null until loaded. */
  fileMetadata: FileMetadata | null;

  // Open lifecycle state (Phase 4 — D-01/D-02)
  /**
   * D-01: Single source of truth for the file open/registration lifecycle.
   * Drives the global blocking overlay, query gating (D-03), and error display.
   * `isLoading` stays QUERY-scoped and is NOT overloaded for the open path.
   */
  registrationStatus: RegistrationStatus;
  /**
   * D-02: Verbatim failure text from the backend Err(String) on registration failure.
   * Rendered as-is in the overlay error state (D-07). Null when no error.
   */
  registrationError: string | null;
  /**
   * Monotonic open token (WR-03). Incremented by setFile and reset so that any
   * in-flight post-open work (e.g. getFileMetadata) can detect whether a newer
   * open superseded it — robust against same-path re-opens, which a filePath
   * equality check cannot distinguish.
   */
  openSeq: number;

  /**
   * D-05 (Phase 5): In-session autofill memory for the remote connection form.
   * Stores the last-entered RemoteConnection values so the form can re-populate
   * without re-entry after app navigation or a failed open attempt.
   *
   * SESSION-SCOPED: this field intentionally survives reset() and setFile() —
   * it is NOT file state. Clearing on reset would break the autofill invariant.
   * Cleared only when the app relaunches (in-memory Zustand store; no persistence).
   *
   * Security note (T-05-05): stored only in the in-memory Zustand store; never
   * written to disk, localStorage, or sessionStorage.
   */
  lastRemoteConnection: RemoteConnection | null;
  setLastRemoteConnection: (conn: RemoteConnection | null) => void;

  // Actions
  setFile: (path: string, schema: SchemaField[]) => void;
  setQueryText: (sql: string) => void;
  setLoading: (loading: boolean) => void;
  /**
   * Store query results after a successful run_query call.
   * Accepts rows directly (decoded from Arrow IPC on the call site).
   */
  setResults: (
    totalRows: number,
    capped: boolean,
    rows: Record<string, unknown>[]
  ) => void;
  /** Set or clear the inline query error (D-01). Pass null to clear. */
  setQueryError: (msg: string | null) => void;
  /**
   * Transient file-open failure toast message (D-02 / FILE-04).
   * Null means no toast; set by drag-drop error path, cleared after auto-dismiss.
   */
  toastMessage: string | null;
  /** Set or clear the transient toast (D-02). Pass null to dismiss. */
  setToast: (msg: string | null) => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  setFileMetadata: (meta: FileMetadata | null) => void;
  /** D-01: Set the open/registration lifecycle status. Used by all three open paths. */
  setRegistrationStatus: (status: RegistrationStatus) => void;
  /** D-02: Set or clear the verbatim registration failure text. Pass null to clear. */
  setRegistrationError: (msg: string | null) => void;
  reset: () => void;
}

export const useAppStore = create<AppState>((set) => ({
  // Initial state
  filePath: null,
  schema: [],
  queryText: DEFAULT_QUERY,
  isLoading: false,
  queryError: null,
  toastMessage: null,
  rows: [],
  totalRows: 0,
  capped: false,
  sidebarCollapsed: false,
  fileMetadata: null,
  registrationStatus: "idle", // D-01: zero value
  registrationError: null, // D-02: no error on start
  openSeq: 0, // WR-03: monotonic open token
  // D-05: session-scoped autofill; NOT cleared by reset/setFile (see field comment)
  lastRemoteConnection: null,

  // Actions
  setFile: (path, schema) =>
    set((state) => ({
      filePath: path,
      schema,
      queryText: DEFAULT_QUERY,
      rows: [],
      totalRows: 0,
      capped: false,
      isLoading: false,
      queryError: null, // D-04: opening a new file starts a clean slate
      fileMetadata: null, // clear stale metadata from previous file
      // sidebarCollapsed: leave as-is (collapse state persists within session per D-02)
      // D-08 (Phase 4): setFile is called only after openFile succeeds, so reaching
      // here means registration succeeded. Set 'registered' to dismiss the overlay.
      registrationStatus: "registered",
      registrationError: null, // clear any previous open error
      openSeq: state.openSeq + 1, // WR-03: this open supersedes any in-flight one
    })),

  // D-04: editing the query clears the inline error — user is actively fixing it.
  setQueryText: (sql) => set({ queryText: sql, queryError: null }),

  setLoading: (loading) => set({ isLoading: loading }),

  setResults: (totalRows, capped, rows) =>
    set({
      rows,
      totalRows,
      capped,
      isLoading: false,
    }),

  setQueryError: (msg) => set({ queryError: msg }),

  setToast: (msg) => set({ toastMessage: msg }),

  setSidebarCollapsed: (collapsed) => set({ sidebarCollapsed: collapsed }),

  setFileMetadata: (meta) => set({ fileMetadata: meta }),

  // D-01 (Phase 4): set the open/registration lifecycle status
  setRegistrationStatus: (status) => set({ registrationStatus: status }),

  // D-02 (Phase 4): set or clear the verbatim registration failure text
  setRegistrationError: (msg) => set({ registrationError: msg }),

  // D-05 (Phase 5): persist or clear in-session autofill values for the remote form
  setLastRemoteConnection: (conn) => set({ lastRemoteConnection: conn }),

  reset: () =>
    set((state) => ({
      filePath: null,
      schema: [],
      queryText: DEFAULT_QUERY,
      isLoading: false,
      queryError: null, // D-04: reset clears all transient error state
      toastMessage: null,
      rows: [],
      totalRows: 0,
      capped: false,
      fileMetadata: null,
      sidebarCollapsed: false,
      registrationStatus: "idle", // D-01: return to zero value
      registrationError: null, // D-02: clear any open error
      openSeq: state.openSeq + 1, // WR-03: invalidate any in-flight open
    })),
}));
