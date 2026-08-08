import { describe, it, expect } from "vitest";
import { detectPlatformString, isMacPlatform, getRunShortcut } from "./platform";

describe("isMacPlatform", () => {
  it("detects 'MacIntel' (WKWebView navigator.platform) as mac", () => {
    expect(isMacPlatform("MacIntel")).toBe(true);
  });

  it("detects 'macOS' (Chromium userAgentData.platform) as mac", () => {
    expect(isMacPlatform("macOS")).toBe(true);
  });

  it("detects a 'Mac' token inside a full userAgent string as mac", () => {
    expect(
      isMacPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"),
    ).toBe(true);
  });

  it("treats 'Win32' as non-mac", () => {
    expect(isMacPlatform("Win32")).toBe(false);
  });

  it("treats 'Windows NT 10.0' as non-mac", () => {
    expect(isMacPlatform("Windows NT 10.0")).toBe(false);
  });

  it("treats 'Linux x86_64' as non-mac", () => {
    expect(isMacPlatform("Linux x86_64")).toBe(false);
  });

  it("treats the empty string as non-mac (safe default)", () => {
    expect(isMacPlatform("")).toBe(false);
  });

  it("falls back to detectPlatformString() when called with no argument", () => {
    // The default-argument path is environment-dependent (Node >= 21 exposes a
    // real navigator.platform, e.g. "MacIntel" on macOS), so assert the
    // contract rather than a hard-coded value: no-arg must equal the explicit
    // call on the detected string, and must always be a boolean.
    const result = isMacPlatform();
    expect(typeof result).toBe("boolean");
    expect(result).toBe(isMacPlatform(detectPlatformString()));
  });
});

describe("detectPlatformString", () => {
  it("always returns a string, never throws", () => {
    expect(typeof detectPlatformString()).toBe("string");
  });
});

describe("getRunShortcut", () => {
  it("returns the Command label on a mac platform string", () => {
    expect(getRunShortcut("MacIntel")).toEqual({
      label: "⌘+Enter",
      ariaLabel: "Command Enter",
    });
  });

  it("returns the Control label on 'Win32'", () => {
    expect(getRunShortcut("Win32")).toEqual({
      label: "Ctrl+Enter",
      ariaLabel: "Control Enter",
    });
  });

  it("returns the Control label for the empty string (non-mac is the safe default)", () => {
    expect(getRunShortcut("")).toEqual({
      label: "Ctrl+Enter",
      ariaLabel: "Control Enter",
    });
  });

  it("keeps label and ariaLabel in sync for a userAgent-shaped mac string", () => {
    const shortcut = getRunShortcut(
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
    );
    expect(shortcut.label).toBe("⌘+Enter");
    expect(shortcut.ariaLabel).toBe("Command Enter");
  });
});
