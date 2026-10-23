import { defineConfig } from "oxfmt";
import ultracite from "ultracite/oxfmt";

export default defineConfig({
  ...ultracite,
  ignorePatterns: [...(ultracite.ignorePatterns ?? [])],
  // Keep Tailwind classes sorted against this project's theme and custom utilities.
  sortTailwindcss: {
    functions: ["clsx", "cva", "tw", "twMerge", "cn", "twJoin", "tv"],
    stylesheet: "./src/styles.css",
  },
});
