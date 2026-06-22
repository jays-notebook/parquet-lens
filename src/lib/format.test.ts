import { describe, it, expect } from "vitest";
import { formatCount, formatBytes } from "./format";

describe("formatCount", () => {
  it("formats zero as '0'", () => {
    expect(formatCount(0)).toBe("0");
  });

  it("formats 1000 with thousands separator", () => {
    // toLocaleString() in Node defaults to en-US — expect comma grouping
    const result = formatCount(1000);
    // Robust: accept both "1,000" (en-US) and any locale that groups thousands
    // Primary assertion: result must contain the digit sequence and a separator
    expect(result).toMatch(/1.000/); // dot or comma separator
  });

  it("formats 1234567 with grouping separators", () => {
    const result = formatCount(1234567);
    // Must contain the seven digits with separators every three digits
    // "1,234,567" (en-US) or equivalent locale grouping
    expect(result).toMatch(/^1.234.567$/);
  });
});

describe("formatBytes", () => {
  it("formats 0 bytes as '0 B'", () => {
    expect(formatBytes(0)).toBe("0 B");
  });

  it("formats 512 bytes as '512 B' (sub-KB)", () => {
    expect(formatBytes(512)).toBe("512 B");
  });

  it("formats exactly 1024 bytes as '1.0 KB' (KB lower boundary)", () => {
    expect(formatBytes(1_024)).toBe("1.0 KB");
  });

  it("formats 1536 bytes as '1.5 KB' (mid-KB value)", () => {
    expect(formatBytes(1_536)).toBe("1.5 KB");
  });

  it("formats exactly 1_048_576 bytes as '1.0 MB' (MB boundary)", () => {
    expect(formatBytes(1_048_576)).toBe("1.0 MB");
  });

  it("formats 4_404_019 bytes as '4.2 MB' (mid-MB value)", () => {
    expect(formatBytes(4_404_019)).toBe("4.2 MB");
  });

  it("formats exactly 1_073_741_824 bytes as '1.0 GB' (GB boundary)", () => {
    expect(formatBytes(1_073_741_824)).toBe("1.0 GB");
  });

  it("formats 2_576_980_378 bytes as '2.4 GB' (mid-GB value)", () => {
    expect(formatBytes(2_576_980_378)).toBe("2.4 GB");
  });
});
