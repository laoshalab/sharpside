import type { Config } from "tailwindcss";

const config: Config = {
  content: ["./src/**/*.{js,ts,jsx,tsx,mdx}"],
  theme: {
    extend: {
      colors: {
        brand: {
          accent: "#00C2A8",
          amber: "#FFB020",
          up: "#00D68F",
          down: "#FF4D6A",
          gold: "#F0C14B",
        },
      },
      fontFamily: {
        sans: [
          '"Avenir Next"',
          '"Segoe UI"',
          '"Helvetica Neue"',
          "ui-sans-serif",
          "system-ui",
          "sans-serif",
        ],
        display: [
          '"Avenir Next Condensed"',
          '"Avenir Next"',
          '"Segoe UI Semibold"',
          "sans-serif",
        ],
        mono: [
          '"Cascadia Code"',
          '"SF Mono"',
          '"JetBrains Mono"',
          "ui-monospace",
          "Menlo",
          "Consolas",
          "monospace",
        ],
      },
      animation: {
        "fade-in": "fadeIn 0.8s ease-out forwards",
        "slide-up": "slideUp 0.8s ease-out forwards",
      },
      keyframes: {
        fadeIn: {
          "0%": { opacity: "0" },
          "100%": { opacity: "1" },
        },
        slideUp: {
          "0%": { opacity: "0", transform: "translateY(30px)" },
          "100%": { opacity: "1", transform: "translateY(0)" },
        },
      },
    },
  },
  plugins: [],
};

export default config;
