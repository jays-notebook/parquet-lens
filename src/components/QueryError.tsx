/**
 * Inline query error display — shown below the SQL editor when a query fails.
 *
 * Renders the engine's verbatim error text (D-03 — raw DataFusion message).
 * Returns null when no error is present so it occupies no space.
 *
 * Placement: rendered between <SqlEditor> and <RunBar> in App.tsx (D-01 inline-below-editor).
 *
 * Styling follows CSS token conventions (PATTERNS.md §CSS Token Usage):
 *   - color: var(--destructive) — error red token; falls back to #dc2626 if absent
 *   - borderTop: 1px solid var(--border) — visual separator from editor above
 *   - monospace 13px, whiteSpace: pre-wrap — preserves DataFusion's line breaks
 *   - flexShrink: 0 — never gets clipped by the vertical flex layout
 *
 * Source: UI-SPEC.md §Inline Error, CONTEXT.md D-01/D-03, QUERY-04
 */
import { useAppStore } from "@/store/appStore";

export function QueryError() {
  // Single-field selector — no useShallow needed (no object reference created).
  const queryError = useAppStore((s) => s.queryError);

  // Mirror LoadingOverlay's early-return shell: occupy no DOM when there is no error.
  if (queryError === null) return null;

  return (
    <div
      role="alert"
      aria-live="polite"
      style={{
        flexShrink: 0,
        borderTop: "1px solid var(--border)",
        padding: "8px 16px",
        fontFamily: "monospace",
        fontSize: "13px",
        whiteSpace: "pre-wrap",
        // --destructive is the standard error color token (shadcn/ui conventions).
        // Fallback to #dc2626 (Tailwind red-600) if the token is absent.
        color: "var(--destructive, #dc2626)",
        backgroundColor: "var(--background)",
        overflowX: "auto",
      }}
    >
      {queryError}
    </div>
  );
}
