/**
 * Number and byte-size formatting helpers (D-08).
 *
 * These are pure functions with no dependencies — safe to import in any component.
 * Formatting decisions (thousands separators, KB/MB/GB auto-scaling) are defined in
 * CONTEXT.md §D-08 and applied to row counts and byte sizes throughout the UI.
 */

/**
 * Formats an integer with thousands separators, e.g. `1234567` → `"1,234,567"`.
 *
 * Used for row counts wherever D-08 applies (file total rows, per-row-group rows).
 */
export function formatCount(n: number): string {
  return Number(n).toLocaleString();
}

/**
 * Formats a byte count with automatic KB/MB/GB scaling (D-08).
 *
 * Thresholds (powers of 1024):
 *   >= 1 GB  → "{x.x} GB"
 *   >= 1 MB  → "{x.x} MB"
 *   >= 1 KB  → "{x.x} KB"
 *   < 1 KB   → "{n} B"
 *
 * Scaled values use one decimal place (`.toFixed(1)`).
 */
export function formatBytes(bytes: number): string {
  if (bytes >= 1_073_741_824) {
    return `${(bytes / 1_073_741_824).toFixed(1)} GB`;
  }
  if (bytes >= 1_048_576) {
    return `${(bytes / 1_048_576).toFixed(1)} MB`;
  }
  if (bytes >= 1_024) {
    return `${(bytes / 1_024).toFixed(1)} KB`;
  }
  return `${bytes} B`;
}
