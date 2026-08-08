import { describe, it, expect } from "vitest";
import { resultsViewState } from "./gridView";

describe("resultsViewState", () => {
  it("shows the welcome card before any query result exists", () => {
    expect(resultsViewState(0, 0, false)).toEqual({
      showEmptyState: true,
      showTable: false,
      showZeroRowNotice: false,
    });
  });

  it("suppresses the welcome card while the first query is in flight", () => {
    expect(resultsViewState(0, 0, true)).toEqual({
      showEmptyState: false,
      showTable: false,
      showZeroRowNotice: false,
    });
  });

  it("shows the table and a zero-row notice when a query returned no rows", () => {
    expect(resultsViewState(0, 3, false)).toEqual({
      showEmptyState: false,
      showTable: true,
      showZeroRowNotice: true,
    });
  });

  it("keeps the headers but hides the notice while a re-run is in flight", () => {
    expect(resultsViewState(0, 3, true)).toEqual({
      showEmptyState: false,
      showTable: true,
      showZeroRowNotice: false,
    });
  });

  it("shows only the table for a result with rows", () => {
    expect(resultsViewState(5, 3, false)).toEqual({
      showEmptyState: false,
      showTable: true,
      showZeroRowNotice: false,
    });
  });

  it("keeps showing the previous result's table while loading", () => {
    expect(resultsViewState(5, 3, true)).toEqual({
      showEmptyState: false,
      showTable: true,
      showZeroRowNotice: false,
    });
  });
});
