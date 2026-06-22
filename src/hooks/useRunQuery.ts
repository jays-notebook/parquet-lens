/**
 * Shared run-query handler (WR-01).
 *
 * Previously duplicated between RunBar (button onClick) and App's
 * useRunQueryHandler (SqlEditor onRun) with divergent error handling.
 * Both now consume this single hook.
 *
 * Gating (D-03/LOAD-02): the handler is a no-op unless a file is registered
 * and no query is already in flight — this guards Ctrl+Enter, which would
 * otherwise bypass the disabled button and query an unregistered table.
 */
import { useCallback } from "react";
import { useShallow } from "zustand/react/shallow";
import { runQuery } from "../lib/tauri";
import { useAppStore } from "../store/appStore";

export function useRunQuery() {
  const {
    queryText,
    isLoading,
    setLoading,
    setResults,
    setQueryError,
    registrationStatus,
  } = useAppStore(
    useShallow((s) => ({
      queryText: s.queryText,
      isLoading: s.isLoading,
      setLoading: s.setLoading,
      setResults: s.setResults,
      setQueryError: s.setQueryError,
      registrationStatus: s.registrationStatus,
    }))
  );

  const runQueryHandler = useCallback(async () => {
    if (isLoading || registrationStatus !== "registered") return;

    setLoading(true);
    try {
      const result = await runQuery(queryText);
      // Successful run: clear any prior inline error before setting results (D-01).
      setQueryError(null);
      setResults(result.total_rows, result.capped, result.rows);
    } catch (err) {
      // Surface the engine's verbatim error inline below the editor (QUERY-04 / D-03).
      setQueryError(err instanceof Error ? err.message : String(err));
    } finally {
      // WR-04: always clear the loading flag, even if setResults or the decode
      // throws — a finally removes the whole class of stuck-loading leaks.
      setLoading(false);
    }
  }, [queryText, isLoading, registrationStatus, setLoading, setResults, setQueryError]);

  return { runQueryHandler };
}
