/**
 * Transient top-right toast for file-open failures (D-02 / FILE-04).
 *
 * Positioned `fixed` at top-right — no layout reflow whatsoever (D-02).
 * Auto-dismisses after ~4 seconds via `setTimeout`/`clearTimeout` in `useEffect`.
 * Renders the raw backend error message verbatim (D-03 — no message rewriting).
 * Returns null when `message` is null — same conditional-render pattern as
 * LoadingOverlay.tsx.
 *
 * `onDismiss` is called by the timer (auto-dismiss) or the close button (manual).
 * The parent is responsible for clearing `toastMessage` in the store.
 *
 * Pattern source: LoadingOverlay.tsx (conditional-render shell, overlay structure)
 */
import { useEffect } from "react";

interface ToastProps {
  message: string | null;
  onDismiss: () => void;
}

export function Toast({ message, onDismiss }: ToastProps) {
  // Auto-dismiss after 4 seconds. Timer resets if a new message arrives.
  useEffect(() => {
    if (!message) return;
    const id = setTimeout(onDismiss, 4000);
    return () => clearTimeout(id);
  }, [message, onDismiss]);

  if (!message) return null;

  return (
    <div
      role="alert"
      aria-live="assertive"
      style={{
        position: "fixed",
        top: "16px",
        right: "16px",
        zIndex: 100,
        backgroundColor: "var(--card)",
        border: "1px solid var(--border)",
        borderRadius: "var(--radius)",
        padding: "12px 16px",
        boxShadow: "0 4px 12px rgba(0, 0, 0, 0.15)",
        maxWidth: "360px",
        color: "var(--foreground)",
        fontSize: "14px",
        // Prevent the toast from growing the page layout (D-02).
        // Fixed positioning already achieves this, but be explicit.
        lineHeight: 1.5,
      }}
    >
      {/* D-03: verbatim error message — no rewriting */}
      {message}
    </div>
  );
}
