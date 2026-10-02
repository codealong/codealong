// The linter checks only what React can't check for itself: that hooks are called the same way
// on every render, and that an effect's or a memo's dependency list names what it reads. Style
// is left to the people writing the code, and types to tsc.
import parser from "@typescript-eslint/parser";
import reactHooks from "eslint-plugin-react-hooks";

export default [
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      parser,
      parserOptions: { ecmaFeatures: { jsx: true } },
    },
    plugins: { "react-hooks": reactHooks },
    // A disable comment that no longer silences anything is an error too, so one can't outlive
    // the code it was written for.
    linterOptions: { reportUnusedDisableDirectives: "error" },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
    },
  },
];
