/**
 * Results-grid view gating (WR-02).
 *
 * The grid must be gated on the RESULT COLUMN COUNT, not the row count.
 *
 * The backend deliberately reports a zero-row result's full column list:
 * `executor.rs` captures `stream.schema()` BEFORE draining the stream, and
 * `test_execute_result_schema_present_for_empty_result` pins that behaviour
 * precisely so the grid can render headers for an empty result. Gating the
 * `<table>` on `rows.length > 0` threw that away — `select * from data where
 * id < 0` fell back to the pre-query welcome card, whose copy ("Open a Parquet
 * file, then run a query to see results here.") is factually wrong after a
 * query has already run (WR-02).
 *
 * # Why column count is the right discriminator
 *
 * `appStore.resultSchema` is `[]` before the first result, and it is cleared by
 * both `setFile` and `reset` — it is only ever populated by `setResults`. So a
 * non-empty column list means exactly one thing: a query result exists for the
 * current file. That is precisely the line between "no query result yet"
 * (welcome card) and "a query returned zero rows" (headers + notice), which the
 * row count alone cannot draw.
 *
 * # Why the notice is suppressed while loading
 *
 * Re-running a query that previously returned zero rows keeps the stale
 * `resultSchema` and `rows` on screen until the new result lands. Without the
 * `isLoading` guard the notice would flash under the loading overlay for a
 * query whose outcome is not yet known.
 *
 * # Why this lives in src/lib/ rather than inline in the component
 *
 * The repo has no jsdom and no React testing library, and adding one is out of
 * scope for this phase. Extracting the decision into a pure function is what
 * makes the behaviour testable at all — the same pattern as `src/lib/format.ts`
 * and its colocated `format.test.ts`.
 */

/** Which of the three mutually-informed grid views should render. */
export interface ResultsViewState {
  /** The pre-first-query welcome card ("No results yet"). */
  showEmptyState: boolean;
  /** The result `<table>`, including its header row. */
  showTable: boolean;
  /** The `Query returned 0 rows.` notice under the headers. */
  showZeroRowNotice: boolean;
}

/**
 * Decides which parts of the results grid render, from the result's column
 * count, its row count, and whether a query is in flight.
 *
 * @param rowCount - decoded result rows currently in the store (`rows.length`)
 * @param columnCount - result-schema columns (`resultSchema.length`)
 * @param isLoading - QUERY-scoped loading flag
 */
export function resultsViewState(
  rowCount: number,
  columnCount: number,
  isLoading: boolean
): ResultsViewState {
  return {
    // Columns exist ⇒ a result exists ⇒ its headers are worth showing.
    showTable: columnCount > 0,
    // No result yet — the welcome card's copy is only true here.
    showEmptyState: columnCount === 0 && !isLoading,
    // A result exists and it genuinely has no rows (not merely pending).
    showZeroRowNotice: columnCount > 0 && rowCount === 0 && !isLoading,
  };
}
