/**
 * Full-window drag-over affordance overlay (D-17).
 *
 * Rendered when the user is dragging a file over the application window.
 * Uses `position: fixed` with inset 0 to cover the entire viewport regardless
 * of which screen (A or B) is currently rendered.
 * `pointerEvents: none` ensures mouse events pass through to the window below
 * (Tauri's own drag-drop handling remains unaffected).
 * `zIndex: 50` places it above all content but below the Toast (zIndex 100).
 *
 * Pattern source: LoadingOverlay.tsx (conditional-render shell, fixed/inset/zIndex/pointerEvents)
 */

interface DragOverlayProps {
  visible: boolean;
}

export function DragOverlay({ visible }: DragOverlayProps) {
  if (!visible) return null;

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        // Semi-transparent backdrop — lets the user see the app behind it.
        backgroundColor: "rgba(0, 0, 0, 0.4)",
        zIndex: 50,
        // Do not capture mouse events — Tauri handles the actual drop event.
        pointerEvents: "none",
      }}
    >
      <span
        style={{
          fontSize: "20px",
          fontWeight: 600,
          color: "#ffffff",
        }}
      >
        Drop a .parquet file here
      </span>
    </div>
  );
}
