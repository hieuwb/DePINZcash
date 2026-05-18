/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        // Zcash gold + zebra monochrome palette.
        zcash: {
          gold: "#F4B728",
          dark: "#0E0E10",
          surface: "#16161A",
          border: "#2A2A30",
          text: "#E6E6EA",
          subtle: "#8C8C95",
          success: "#4ADE80",
          warn: "#F59E0B",
          danger: "#EF4444",
        },
      },
      fontFamily: {
        sans: [
          "Inter",
          "system-ui",
          "-apple-system",
          "Segoe UI",
          "Helvetica",
          "Arial",
          "sans-serif",
        ],
        mono: [
          "ui-monospace",
          "SFMono-Regular",
          "Menlo",
          "monospace",
        ],
      },
    },
  },
  plugins: [],
};
