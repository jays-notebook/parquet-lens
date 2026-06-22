// Tailwind CSS v4 uses CSS-first configuration.
// This file is kept for tooling compatibility but the actual configuration
// lives in src/index.css via @import "tailwindcss" + CSS variables.
//
// Source: UI-SPEC.md §Design System
import type { Config } from "tailwindcss";

export default {
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx,js,jsx}"],
} satisfies Config;
