export default [
  {
    files: ["**/*.{js,mjs,cjs}"],
    languageOptions: { ecmaVersion: "latest", sourceType: "module" },
    rules: {
      "complexity": ["error", 15],
      "max-lines-per-function": ["error", { max: 80, skipBlankLines: true, skipComments: true }],
      "id-length": ["error", { min: 2, properties: "never", exceptions: ["i", "j", "x", "y"] }],
      "no-unused-vars": ["error", { args: "all", argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
      "no-magic-numbers": ["error", { ignore: [-1, 0, 1], ignoreArrayIndexes: true, ignoreDefaultValues: true }],
      "for-direction": "error",
      "no-restricted-syntax": ["error", { selector: "ForStatement[test=null]", message: "Use an explicit loop condition or document an exception." }],
    },
  },
];
