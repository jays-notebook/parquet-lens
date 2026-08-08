/**
 * Results grid — renders query results decoded from Arrow IPC.
 *
 * Phase 1 (Task 3): TanStack Table 8 with name-only column headers,
 * NULL as blank, pre-calculated column widths, basic horizontal scroll.
 *
 * Phase 2 (Plan 03): Two-line typed headers (GRID-02), header tooltips (GRID-06),
 * italic gray NULL marker distinct from blank empty strings (GRID-04).
 *
 * Phase 1 (Plan 01-01): Columns now derive from the query RESULT schema
 * (`appStore.resultSchema`, supplied by the backend on the metadata channel)
 * rather than the FILE schema (`appStore.schema`). Reading the file schema here
 * is what made aggregates, aliases and computed columns render as all-NULL rows
 * under the file's column names (RESULT-01/03/04). `appStore.schema` remains the
 * sidebar's source (SchemaTab / SchemaPanel) and must NOT be read in this file.
 *
 * Column ids and row keys are the DEDUPED keys from `dedupeFieldKeys`, so two
 * result columns with the same name stay independent (D-PH1-03); headers still
 * display the original un-deduped `SchemaField.name`.
 *
 * Phase 1 (Plan 01-02): headers and tooltips render a DISPLAY name via
 * `displayColumnName` (which cleans DataFusion's `count(*)` to `count`, RESULT-02)
 * while ids and accessors keep using the deduped RAW keys. The two must not be
 * unified: row objects are keyed by the raw backend names, so a display name in an
 * id or accessor would resolve to `undefined` and render every cell as NULL.
 *
 * Phase 2 (Plan 02-01): the table is gated on the RESULT COLUMN COUNT via
 * `resultsViewState`, not on the row count, so a query returning zero rows still
 * renders its headers plus a `Query returned 0 rows.` notice instead of falling
 * back to the pre-query welcome card (WR-02). Column definitions and their
 * deduped keys are memoized on `resultSchema`, so `useReactTable` receives a
 * stable `columns` reference across renders (IN-04).
 *
 * Source: STACK.md §TanStack Table + Virtual, UI-SPEC.md §Results Grid
 */

import { useMemo } from "react";
import {
  flexRender,
  getCoreRowModel,
  useReactTable,
  type ColumnDef,
} from "@tanstack/react-table";
import { dedupeFieldKeys, displayColumnName, type SchemaField } from "@/lib/tauri";
import { resultsViewState } from "@/lib/gridView";
import { useShallow } from "zustand/react/shallow";
import { useAppStore } from "@/store/appStore";
import { LoadingOverlay } from "@/components/LoadingOverlay";

export function ResultsGrid() {
  const { rows, isLoading, resultSchema } = useAppStore(useShallow((s) => ({
    rows: s.rows,
    isLoading: s.isLoading,
    resultSchema: s.resultSchema,
  })));

  // Same positional field-name list `arrowTableToRows` keyed the rows with, so
  // column ids and row keys are guaranteed to agree (D-PH1-03).
  const columnKeys = useMemo(
    () => dedupeFieldKeys(resultSchema.map((f) => f.name)),
    [resultSchema]
  );

  const columns: ColumnDef<Record<string, unknown>, unknown>[] = useMemo(
    () => resultSchema.map((field, i) => ({
    id: columnKeys[i],
    // accessorFn, NOT accessorKey: TanStack treats accessorKey as a dot-delimited
    // deep path, so a result column named e.g. `sum(a.b)` — which DataFusion will
    // happily emit — would silently resolve to undefined and render as NULL.
    accessorFn: (row: Record<string, unknown>) => row[columnKeys[i]],
    // D-10: Two-line header — column name (bold) over Arrow-native type (dimmed).
    // Arrow-native type names are locked from Phase 1 (no SQL aliases).
    header: () => (
      <div style={{ lineHeight: 1.3 }}>
        <div style={{ fontWeight: 600, color: "var(--foreground)" }}>
          {displayColumnName(field.name)}
        </div>
        <div style={{ fontSize: "11px", color: "var(--muted-foreground)", fontWeight: 400 }}>
          {field.arrow_type}
        </div>
      </div>
    ),
    // Store field in meta so the <th> tooltip can access it in the thead loop.
    meta: { field } as { field: SchemaField },
    // Pre-calculated column widths by Arrow type (PITFALLS.md §Pitfall 5).
    size: getColumnWidth(field.arrow_type),
    // D-09: NULL (undefined) renders as italic gray NULL; empty string renders blank.
    cell: ({ getValue }) => {
      const value = getValue();
      if (value === undefined || value === null) {
        return (
          <span style={{ fontStyle: "italic", color: "var(--muted-foreground)" }}>
            NULL
          </span>
        );
      }
      return String(value);
    },
    })),
    [resultSchema, columnKeys]
  );

  const table = useReactTable({
    data: rows,
    columns,
    getCoreRowModel: getCoreRowModel(),
  });

  const view = resultsViewState(rows.length, resultSchema.length, isLoading);

  return (
    <div
      role="grid"
      aria-live="polite"
      aria-busy={isLoading}
      style={{
        flex: 1,
        position: "relative",
        overflow: "auto",
        backgroundColor: "var(--background)",
        minHeight: 0,
      }}
    >
      <LoadingOverlay visible={isLoading} />

      <div style={{ opacity: isLoading ? 0.3 : 1, minHeight: "100%" }}>
        {view.showEmptyState && (
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              justifyContent: "center",
              height: "100%",
              minHeight: "200px",
              gap: "8px",
              color: "var(--muted-foreground)",
            }}
          >
            <p
              style={{
                fontSize: "16px",
                fontWeight: 600,
                margin: 0,
                color: "var(--foreground)",
              }}
            >
              No results yet
            </p>
            <p style={{ fontSize: "14px", margin: 0, textAlign: "center" }}>
              Open a Parquet file, then run a query to see results here.
            </p>
          </div>
        )}

        {view.showTable && (
          <table
            style={{
              borderCollapse: "collapse",
              width: "max-content",
              minWidth: "100%",
              fontSize: "14px",
            }}
          >
            <thead>
              {table.getHeaderGroups().map((headerGroup) => (
                <tr key={headerGroup.id}>
                  {headerGroup.headers.map((header) => {
                    // D-11: Read field from meta for the native tooltip (zero extra deps).
                    const meta = header.column.columnDef.meta as { field: SchemaField } | undefined;
                    const f = meta?.field;
                    const titleText = f
                      ? `${displayColumnName(f.name)} (${f.arrow_type}${f.nullable ? ", nullable" : ", not null"})`
                      : undefined;
                    return (
                    <th
                      key={header.id}
                      role="columnheader"
                      title={titleText}
                      style={{
                        width: `${header.getSize()}px`,
                        minWidth: `${header.getSize()}px`,
                        textAlign: "left",
                        padding: "6px 8px",
                        fontSize: "12px",
                        fontWeight: 400,
                        color: "var(--muted-foreground)",
                        backgroundColor: "var(--muted)",
                        borderBottom: "1px solid var(--border)",
                        borderRight: "1px solid var(--border)",
                        position: "sticky",
                        top: 0,
                        // Allow header row to grow vertically to fit two-line content (D-10).
                        // Keep horizontal overflow hidden for very long names.
                        overflow: "hidden",
                      }}
                    >
                      {flexRender(
                        header.column.columnDef.header,
                        header.getContext()
                      )}
                    </th>
                    );
                  })}
                </tr>
              ))}
            </thead>
            <tbody>
              {table.getRowModel().rows.map((row) => (
                <tr key={row.id}>
                  {row.getVisibleCells().map((cell) => (
                    <td
                      key={cell.id}
                      style={{
                        width: `${cell.column.getSize()}px`,
                        minWidth: `${cell.column.getSize()}px`,
                        padding: "6px 8px",
                        fontSize: "14px",
                        fontWeight: 400,
                        color: "var(--foreground)",
                        borderBottom: "1px solid var(--border)",
                        borderRight: "1px solid var(--border)",
                        whiteSpace: "nowrap",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        maxWidth: "300px",
                      }}
                    >
                      {flexRender(
                        cell.column.columnDef.cell,
                        cell.getContext()
                      )}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        )}

        {view.showZeroRowNotice && (
          <div
            style={{
              padding: "12px 8px",
              fontSize: "14px",
              color: "var(--muted-foreground)",
            }}
          >
            <p style={{ margin: 0 }}>Query returned 0 rows.</p>
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * Pre-calculate column width from Arrow type string (PITFALLS.md §Pitfall 5).
 * Avoids `width: auto` which causes layout thrashing in virtualized tables.
 */
function getColumnWidth(arrowType: string): number {
  const t = arrowType.toLowerCase();
  if (t.startsWith("int") || t.startsWith("uint") || t.startsWith("float")) {
    return 80;
  }
  if (t.startsWith("utf8") || t.startsWith("largeutf8")) {
    return 150;
  }
  if (t.startsWith("bool")) {
    return 60;
  }
  if (t.startsWith("timestamp") || t.startsWith("date")) {
    return 120;
  }
  return 100;
}
