import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./tests/setup.ts"],
    css: false,
    exclude: ["**/node_modules/**", "**/a11y.test.tsx", "**/a11y.game.test.tsx", "**/tests/e2e/**"],
    coverage: {
      provider: "v8",
      include: ["components/OutcomeChip.tsx", "components/WagerInput.tsx", "components/SideSelector.tsx", "components/GameStateCard.tsx"],
      thresholds: { lines: 80, branches: 80 },
    },
  },
});
