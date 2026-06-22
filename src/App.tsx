/**
 * Root application component — two-screen switcher.
 *
 * Screen A (filePath === null): full-window welcome card (OpenScreen).
 * Screen B (filePath !== null): vertical stack per UI-SPEC.md §Layout Contract:
 *   [Toolbar 48px] [SqlEditor 160px] [RunBar 40px]
 *   [RowCapChip 32px conditional] [ResultsGrid flex-grow]
 *
 * Window-level drag-drop listener is mounted here so it covers BOTH Screen A
 * and Screen B (D-14 whole-window scope). The listener is set up once and
 * remains active regardless of which screen is rendered.
 *
 * Source: UI-SPEC.md §Layout Contract, §Screen B
 */
import { useState, useEffect } from "react";
import { useShallow } from "zustand/react/shallow";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useAppStore } from "./store/appStore";
import { useOpenFile } from "./hooks/useOpenFile";
import { useRunQuery } from "./hooks/useRunQuery";
import { OpenScreen } from "./components/OpenScreen";
import { Toolbar } from "./components/Toolbar";
import { SqlEditor } from "./components/SqlEditor";
import { RunBar } from "./components/RunBar";
import { QueryError } from "./components/QueryError";
import { Toast } from "./components/Toast";
import { DragOverlay } from "./components/DragOverlay";
import { RegistrationOverlay } from "./components/RegistrationOverlay";
import { RemoteConnectionModal } from "./components/RemoteConnectionModal";
import { SchemaPanel } from "./components/SchemaPanel";

// Task 3 components — imported here to complete Screen B; they are implemented in Task 3.
// Slots are wired now so Task 3 drops them in without relayout.
import { ResultsGrid } from "./components/ResultsGrid";
import { RowCapChip } from "./components/RowCapChip";

export default function App() {
  const { filePath, queryText, isLoading, totalRows, setQueryText } =
    useAppStore(useShallow((s) => ({
      filePath: s.filePath,
      queryText: s.queryText,
      isLoading: s.isLoading,
      totalRows: s.totalRows,
      setQueryText: s.setQueryText,
    })));

  const { toastMessage, setToast, registrationStatus } = useAppStore(
    useShallow((s) => ({
      toastMessage: s.toastMessage,
      setToast: s.setToast,
      registrationStatus: s.registrationStatus,
    }))
  );

  // Shared open + run pipelines (WR-01/WR-02) — single implementation each.
  const { openPath, openRemote } = useOpenFile();

  // Phase 5 (REMOTE-01): remote connection modal visibility state
  const [showRemoteModal, setShowRemoteModal] = useState(false);
  const { runQueryHandler } = useRunQuery();

  // Window-level drag-over state — drives DragOverlay visibility (D-17).
  const [dragOver, setDragOver] = useState(false);

  // Register the window-level Tauri drag-drop event listener once on mount.
  // Active on both Screen A and Screen B (D-14 whole-window scope).
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over" || event.payload.type === "enter") {
          setDragOver(true); // show full-window overlay (D-17)
        } else if (event.payload.type === "leave") {
          setDragOver(false);
        } else if (event.payload.type === "drop") {
          setDragOver(false);
          const paths = event.payload.paths;
          if (paths.length === 0) {
            // Tauri can emit a drop with no resolvable file paths (e.g. dropping
            // non-file content). Nothing droppable — do not enter the
            // registration lifecycle with an undefined path (CR-01).
            return;
          }
          if (paths.length > 1) {
            // D-15 / D-05: hard blocking alert for multi-file drops.
            // Deliberate exception to the toast convention — uses alert(), not toast.
            window.alert(
              "Please drop a single .parquet file. Multiple files are not supported."
            );
            return;
          }
          void openPath(paths[0]);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });

    return () => {
      unlisten?.();
    };
    // openPath is a stable useCallback (its deps are all stable Zustand actions),
    // so the listener registers exactly once.
  }, [openPath]);

  // Root-level overlays — rendered before the screen branch so they appear on
  // both Screen A and Screen B (D-14 whole-window scope).
  const rootOverlays = (
    <>
      <Toast message={toastMessage} onDismiss={() => setToast(null)} />
      <DragOverlay visible={dragOver} />
      {/* Phase 4 (LOAD-01): global blocking overlay for file open/registration lifecycle */}
      <RegistrationOverlay />
      {/* Phase 5 (REMOTE-01): remote connection modal — zIndex 70, above RegistrationOverlay */}
      <RemoteConnectionModal
        open={showRemoteModal}
        onClose={() => setShowRemoteModal(false)}
        onSubmit={openRemote}
      />
    </>
  );

  // inert attribute value: truthy "" blocks all keyboard/pointer events to background;
  // undefined removes the attribute entirely (React boolean-attr convention).
  // Also inert while the remote modal is open to prevent background interaction (D-03).
  const backgroundInert =
    registrationStatus === "registering" ||
    registrationStatus === "error" ||
    showRemoteModal
      ? ""
      : undefined;

  if (filePath === null) {
    // Screen A — welcome / file-open screen
    return (
      <>
        {rootOverlays}
        {/* inert focus trap: blocks all interaction while overlay is active (LOAD-01 / accessibility) */}
        <div inert={backgroundInert}>
          <OpenScreen onOpenRemote={() => setShowRemoteModal(true)} />
        </div>
      </>
    );
  }

  // Screen B — horizontal split: [left sidebar | right vertical stack] (D-01)
  return (
    <>
      {rootOverlays}
      <div
        inert={backgroundInert}
        style={{
          display: "flex",
          flexDirection: "row",
          height: "100vh",
          overflow: "hidden",
          backgroundColor: "var(--background)",
        }}
      >
        {/* [Left] Schema/Metadata sidebar (D-01 / D-04 — Screen B only) */}
        <SchemaPanel />

        {/* [Right] Existing vertical stack — flex:1, minWidth:0 to prevent overflow */}
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            flex: 1,
            minWidth: 0,
            overflow: "hidden",
          }}
        >
          {/* [1] Toolbar — 48px */}
          <Toolbar onOpenRemote={() => setShowRemoteModal(true)} />

          {/* [2] SQL Editor — 160px fixed */}
          <SqlEditor
            value={queryText}
            onChange={setQueryText}
            onRun={runQueryHandler}
            disabled={isLoading}
          />

          {/* [2b] Inline query error — shown directly below editor (D-01 / QUERY-04) */}
          <QueryError />

          {/* [3] Run Bar — 40px */}
          <RunBar />

          {/* [4] Row Cap Chip — 32px, shown only when results exist */}
          {totalRows > 0 && <RowCapChip />}

          {/* [5] Results Grid — flex-grow */}
          <ResultsGrid />
        </div>
      </div>
    </>
  );
}
