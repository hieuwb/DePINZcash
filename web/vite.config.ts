import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Match DePIN-Web convention so backend CORS env vars are reusable.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 3002,
    host: true,
  },
  preview: {
    port: 3002,
  },
});
