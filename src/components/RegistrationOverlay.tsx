/**
 * Global blocking overlay for the file open/registration lifecycle (LOAD-01).
 *
 * Renders on top of ALL content (zIndex: 60) when registrationStatus is
 * 'registering' or 'error'. backdrop pointer-events: all blocks all background
 * interaction — the opposite of DragOverlay (pointer-events: none).
 *
 * Loading state: shows a Loader2 32px spinner + "Opening file…" label.
 * Error state: shows AlertCircle + "Failed to open file" heading + verbatim
 * registrationError body + "Open another file" dismiss button.
 *
 * Dismiss restores registrationStatus conditionally (D-04/D-05):
 *   - If a file is still open (filePath !== null) → 'registered', so the
 *     preserved previous DataFusion table remains queryable after a failed
 *     re-open attempt (the backend register_source is atomic — D-04/D-05).
 *   - If no file is open (first action was a bad drop) → 'idle', returning
 *     to the welcome screen (Screen A).
 * Always clears registrationError.
 *
 * Focus trap: background is made inert via App.tsx; on entering the
 * registering state focus moves to the loading container, and on the error
 * state to the dismiss button — so focus is deterministically inside the
 * active overlay for both states rather than stranded on the now-inert
 * background (accessibility — D-03 / WR-05).
 *
 * Source: UI-SPEC.md §RegistrationOverlay, §Overlay visual states,
 *         §Copywriting Contract, §Accessibility Contract
 * Pattern: DragOverlay.tsx (shell), LoadingOverlay.tsx (Loader2 spin),
 *          Toast.tsx (verbatim message rendering)
 */
import { useEffect, useRef } from "react";
import { Loader2, AlertCircle } from "lucide-react";
import { Button } from "./ui/button";
import { useAppStore } from "../store/appStore";

export function RegistrationOverlay() {
  const registrationStatus = useAppStore((s) => s.registrationStatus);
  const registrationError = useAppStore((s) => s.registrationError);
  const setRegistrationStatus = useAppStore((s) => s.setRegistrationStatus);
  const setRegistrationError = useAppStore((s) => s.setRegistrationError);

  // Dismiss button ref — focus is moved here when error state mounts (accessibility).
  const dismissButtonRef = useRef<HTMLButtonElement>(null);
  // Loading container ref — focus is moved here while registering (WR-05).
  const loadingRef = useRef<HTMLDivElement>(null);

  // Move focus into the active overlay so it never stays on the inert
  // background (D-03 / WCAG / WR-05): dismiss button on error, loading
  // container while registering.
  useEffect(() => {
    if (registrationStatus === "error") {
      dismissButtonRef.current?.focus();
    } else if (registrationStatus === "registering") {
      loadingRef.current?.focus();
    }
  }, [registrationStatus]);

  // Return null when idle or registered — no DOM node, no layout cost.
  if (registrationStatus !== "registering" && registrationStatus !== "error") {
    return null;
  }

  function handleDismiss() {
    // D-04/D-05: Restore the correct status depending on whether a file is still
    // registered in DataFusion. register_source is atomic (build-then-swap), so
    // a failed re-open leaves the previous table intact and queryable.
    // Reading filePath via getState() avoids a reactive selector just for a click handler.
    const hasPreviousFile = useAppStore.getState().filePath !== null;
    setRegistrationStatus(hasPreviousFile ? "registered" : "idle");
    setRegistrationError(null);
  }

  // Shared backdrop + flex-center container (pointer-events: all blocks background).
  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        // More opaque than DragOverlay (0.4) — signals a hard block.
        backgroundColor: "rgba(0, 0, 0, 0.65)",
        zIndex: 60, // above DragOverlay (50), below Toast (100)
        // Opposite of DragOverlay — captures all pointer events to block background.
        pointerEvents: "all",
      }}
    >
      {registrationStatus === "registering" ? (
        // Loading state: spinner + label. Not dismissable.
        <div
          ref={loadingRef}
          // tabIndex={-1} makes the container programmatically focusable (WR-05)
          // without inserting it into the tab order. outline:none avoids a focus
          // ring on this non-interactive container.
          tabIndex={-1}
          aria-live="polite"
          aria-busy="true"
          style={{
            outline: "none",
            backgroundColor: "var(--card)",
            width: "400px",
            borderRadius: "var(--radius)",
            padding: "24px",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            gap: "8px",
          }}
        >
          <Loader2
            size={32}
            className="animate-spin"
            style={{ color: "var(--muted-foreground)" }}
          />
          {/* "Opening file…" — exact copy per UI-SPEC.md §Copywriting Contract */}
          <span
            style={{
              fontSize: "14px",
              fontWeight: 400,
              color: "var(--muted-foreground)",
              lineHeight: 1.5,
            }}
          >
            Opening file…
          </span>
        </div>
      ) : (
        // Error state: icon + heading + verbatim error body + dismiss button.
        <div
          role="alertdialog"
          aria-modal="true"
          aria-describedby="registration-error-body"
          style={{
            backgroundColor: "var(--card)",
            width: "400px",
            borderRadius: "var(--radius)",
            padding: "24px",
            // Top border signals error state (UI-SPEC.md §Overlay visual states).
            borderTop: "1px solid var(--destructive)",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            gap: "12px",
          }}
        >
          {/* Error icon: AlertCircle 24px in --destructive color */}
          <AlertCircle
            size={24}
            style={{ color: "var(--destructive)", flexShrink: 0 }}
          />

          {/* Heading: "Failed to open file" — exact copy per UI-SPEC.md §Copywriting Contract */}
          <span
            style={{
              fontSize: "16px",
              fontWeight: 600,
              color: "var(--foreground)",
              lineHeight: 1.2,
              textAlign: "center",
            }}
          >
            Failed to open file
          </span>

          {/* Verbatim error body — D-07: no rewriting, render as-is (React escapes HTML) */}
          <div
            id="registration-error-body"
            style={{
              fontSize: "14px",
              fontWeight: 400,
              color: "var(--muted-foreground)",
              lineHeight: 1.5,
              maxHeight: "120px",
              overflowY: "auto",
              width: "100%",
              textAlign: "center",
            }}
          >
            {registrationError}
          </div>

          {/* Dismiss button: "Open another file" — full-width, 44px min-height (touch target) */}
          <Button
            ref={dismissButtonRef}
            onClick={handleDismiss}
            style={{
              width: "100%",
              minHeight: "44px",
            }}
          >
            Open another file
          </Button>
        </div>
      )}
    </div>
  );
}
