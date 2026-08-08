/**
 * Platform detection helpers for keyboard-shortcut labels.
 *
 * These are pure, dependency-free functions whose only job is to keep the
 * *displayed* run-shortcut label in sync with CodeMirror's `Mod-Enter` binding
 * (`src/components/SqlEditor.tsx`). CodeMirror resolves `Mod` to Cmd on macOS
 * and Ctrl everywhere else, so a hardcoded "Ctrl+Enter" hint reads as a broken
 * shortcut to macOS users even though the binding itself is correct.
 *
 * The detected platform string is never rendered, logged, or sent over IPC — it
 * only selects between two hardcoded literal labels.
 *
 * Every function takes the platform string as an explicit (defaulted) argument
 * so the decision logic is unit-testable without touching a real `navigator`.
 */

/**
 * The two strings that describe the run shortcut on the current platform.
 *
 * `ariaLabel` exists because a bare `⌘` glyph is announced unhelpfully by screen
 * readers; both strings live together so they cannot drift apart.
 */
export interface RunShortcut {
  /** Visible hint text, e.g. `"⌘+Enter"` or `"Ctrl+Enter"`. */
  label: string;
  /** Screen-reader text, e.g. `"Command Enter"` or `"Control Enter"`. */
  ariaLabel: string;
}

/**
 * Reads the host platform defensively and returns a single string to match against.
 *
 * Resolution order, first non-empty wins:
 *   1. `navigator.userAgentData.platform` — Chromium / WebView2 reports `"macOS"` / `"Windows"`
 *   2. `navigator.platform` — WKWebView reports `"MacIntel"`
 *   3. `navigator.userAgent` — last-resort fallback containing a `Macintosh` token
 *
 * Returns `""` when there is no `navigator` at all (e.g. a bare SSR/node context),
 * which callers treat as non-mac.
 */
export function detectPlatformString(): string {
  if (typeof navigator === "undefined") return "";

  // `userAgentData` is not in the configured DOM lib typings — reach it through a
  // local inline cast rather than widening `navigator` or adding a global declare.
  const nav = navigator as Navigator & {
    userAgentData?: { platform?: string };
  };

  return nav.userAgentData?.platform || nav.platform || nav.userAgent || "";
}

/**
 * Case-insensitive test for a `mac` token in the given platform string.
 *
 * Matches `"MacIntel"`, `"macOS"` and the `"Macintosh"` token inside a full
 * userAgent. Anything else — including the empty string — is non-mac, which is
 * the safe default because Ctrl+Enter is the majority-platform binding.
 */
export function isMacPlatform(platform: string = detectPlatformString()): boolean {
  return /mac/i.test(platform);
}

/**
 * Returns the run-shortcut label pair for the given platform.
 *
 * macOS → `{ label: "⌘+Enter", ariaLabel: "Command Enter" }`,
 * everything else → `{ label: "Ctrl+Enter", ariaLabel: "Control Enter" }`.
 *
 * This mirrors CodeMirror's `Mod-Enter` resolution — it does not change the
 * binding, it only describes it.
 */
export function getRunShortcut(platform: string = detectPlatformString()): RunShortcut {
  return isMacPlatform(platform)
    ? { label: "⌘+Enter", ariaLabel: "Command Enter" }
    : { label: "Ctrl+Enter", ariaLabel: "Control Enter" };
}
