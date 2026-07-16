import expandNestedObjectExpression from "./eslint-rules/expand-nested-object-expression.js";
import expandNestedTypeLiteral from "./eslint-rules/expand-nested-type-literal.js";
import expandSvelteBlock from "./eslint-rules/expand-svelte-block.js";
import multilineArgParenNewline from "./eslint-rules/multiline-arg-paren-newline.js";
import noPaddedTag from "./eslint-rules/no-padded-tag.js";
import stylistic from "@stylistic/eslint-plugin";
import importNewlines from "eslint-plugin-import-newlines";
import perfectionist from "eslint-plugin-perfectionist";
import svelteEslint from "eslint-plugin-svelte";
import { globalIgnores } from "eslint/config";
import globals from "globals";
import svelteParser from "svelte-eslint-parser";
import tsEslint from "typescript-eslint";

// Logic and correctness rules live in oxlint (.oxlintrc.json) - ESLint only
// carries what oxlint cannot: formatting, Svelte template rules, selector
// bans and the local custom rules
const tsStyleRules = {
  "perfectionist/sort-imports": [
    "error",
    {
      type: "alphabetical",
      order: "asc",
      newlinesBetween: "ignore",
      sortSideEffects: true,
      groups: [["side-effect", "builtin", "external", "internal", "parent", "sibling", "index", "unknown"]]
    }
  ],
  "perfectionist/sort-named-imports": ["error", {
    type: "alphabetical",
    order: "asc",
    ignoreCase: true
  }],
  "perfectionist/sort-named-exports": ["error", {
    type: "alphabetical",
    order: "asc",
    ignoreCase: true
  }],
  "@stylistic/quotes": ["error", "double", { allowTemplateLiterals: "always" }],
  "@stylistic/quote-props": ["error", "as-needed"],
  "@stylistic/semi": ["error"],
  "@typescript-eslint/naming-convention": [
    "error",
    {
      selector: "interface",
      format: ["PascalCase"],
      custom: {
        regex: "^I[A-Z]",
        match: false
      }
    }
  ],
  "@stylistic/indent": ["error", 2],
  "@stylistic/arrow-parens": ["error", "as-needed"],
  "@stylistic/object-curly-spacing": ["error", "always"],
  "@stylistic/brace-style": "error",
  "@stylistic/comma-dangle": ["error", "never"],
  "@stylistic/no-trailing-spaces": "error",
  "@stylistic/eol-last": ["error", "always"],
  "@stylistic/no-multiple-empty-lines": [
    "error",
    {
      max: 1,
      maxEOF: 0,
      maxBOF: 0
    }
  ],
  "@stylistic/comma-spacing": ["error", {
    before: false,
    after: true
  }],
  "@stylistic/key-spacing": ["error", {
    beforeColon: false,
    afterColon: true
  }],
  "@stylistic/keyword-spacing": ["error", {
    before: true,
    after: true
  }],
  "@stylistic/space-before-blocks": "error",
  "@stylistic/space-before-function-paren": [
    "error",
    {
      named: "never",
      asyncArrow: "always",
      catch: "always"
    }
  ],
  "@stylistic/space-infix-ops": "error",
  "@stylistic/space-in-parens": ["error", "never"],
  "@stylistic/array-bracket-spacing": ["error", "never"],
  "@stylistic/computed-property-spacing": ["error", "never"],
  "@stylistic/template-curly-spacing": ["error", "never"],
  "@stylistic/block-spacing": ["error", "always"],
  "@stylistic/semi-spacing": ["error", {
    before: false,
    after: true
  }],
  "@stylistic/no-extra-semi": "error",
  "@stylistic/type-annotation-spacing": "error",
  "@stylistic/member-delimiter-style": [
    "error",
    {
      multiline: {
        delimiter: "semi",
        requireLast: true
      },
      singleline: {
        delimiter: "semi",
        requireLast: false
      }
    }
  ],
  "@stylistic/no-mixed-spaces-and-tabs": "error",
  "@stylistic/no-tabs": "error",
  "@stylistic/max-len": [
    "error",
    {
      code: 120,
      ignoreUrls: true,
      ignoreStrings: true,
      ignoreTemplateLiterals: true,
      ignoreRegExpLiterals: true,
      ignorePattern: "d=\"[^\"]*\""
    }
  ],
  "@stylistic/padded-blocks": ["error", "never"],
  "@stylistic/rest-spread-spacing": ["error", "never"],
  "@stylistic/spaced-comment": ["error", "always"],
  "import-newlines/enforce": [
    "error",
    {
      items: 4,
      "max-len": 120,
      forceSingleLine: true
    }
  ],
  "@stylistic/object-curly-newline": [
    "error",
    {
      ObjectExpression: {
        consistent: true,
        multiline: true
      },
      ObjectPattern: {
        consistent: true,
        multiline: true
      },
      ExportDeclaration: {
        consistent: true,
        multiline: true
      }
    }
  ],
  "@stylistic/object-property-newline": ["error", { allowAllPropertiesOnSameLine: false }],
  "@stylistic/padding-line-between-statements": [
    "error",
    {
      blankLine: "always",
      prev: "import",
      next: ["const", "let", "function", "export", "type"]
    },
    {
      blankLine: "any",
      prev: "import",
      next: "import"
    },
    {
      blankLine: "always",
      prev: "*",
      next: "if"
    },
    {
      blankLine: "never",
      prev: ["const", "let"],
      next: "if"
    },
    {
      blankLine: "always",
      prev: "if",
      next: "*"
    }
  ],
  "perfectionist/sort-objects": [
    "error",
    {
      type: "unsorted",
      newlinesBetween: 0
    }
  ],
  "@stylistic/function-call-argument-newline": ["error", "consistent"],
  "@stylistic/function-paren-newline": ["error", "consistent"],
  "local/expand-nested-object-expression": "error",
  "local/expand-nested-type-literal": "error",
  "local/multiline-arg-paren-newline": "error",
  "local/no-padded-tag": "error",
  // Arrow-function-to-variable is enforced by oxlint's func-style (allowArrowFunctions:
  // false), but that only catches a direct `const x = () => ...` - a ternary that
  // picks between two arrows evades it, so the selectors below close that gap.
  "no-restricted-syntax": [
    "error",
    {
      selector: "ForOfStatement > CallExpression[callee.object.name='Object'][callee.property.name='keys']",
      message: "Use a for-in loop instead of for-of Object.keys()."
    },
    {
      selector: "MemberExpression[object.name='Reflect']",
      message: "Do not use Reflect. Use direct property access instead."
    },
    {
      selector: "VariableDeclarator[init.type='ConditionalExpression'][init.consequent.type='ArrowFunctionExpression']",
      message: "Declare a named function instead of assigning an arrow function to a variable - fold the condition inside the function body."
    },
    {
      selector: "VariableDeclarator[init.type='ConditionalExpression'][init.alternate.type='ArrowFunctionExpression']",
      message: "Declare a named function instead of assigning an arrow function to a variable - fold the condition inside the function body."
    },
    {
      // Svelte content tags only: a `{cond ? a : b}` that outputs text. Use an {#if}/{:else}
      // block (or a $derived.by with if/else for a value) so the branch reads as markup. Scoped
      // to content mustaches - attribute/directive value ternaries (attr={a ? b : c}) are fine.
      selector: "SvelteMustacheTag:not([parent.type='SvelteAttribute']):not([parent.type='SvelteStyleDirective']) > ConditionalExpression",
      message: "Do not use a ternary in a template tag - use an {#if}/{:else} block instead."
    }
  ]
};

const sharedPlugins = {
  "@stylistic": stylistic,
  "@typescript-eslint": tsEslint.plugin,
  "import-newlines": importNewlines,
  perfectionist,
  local: {
    rules: {
      "expand-nested-object-expression": expandNestedObjectExpression,
      "expand-nested-type-literal": expandNestedTypeLiteral,
      "expand-svelte-block": expandSvelteBlock,
      "multiline-arg-paren-newline": multilineArgParenNewline,
      "no-padded-tag": noPaddedTag
    }
  }
};

const browserGlobals = {
  ...globals.browser
};

// Node globals belong only to tooling that actually runs under Node (config
// files, the scripts/ CLI helpers). Vite/webview source must not see `process`
// - oxlint enforces this via the matching env override.
const nodeToolingFiles = ["**/*.config.ts", "**/*.config.js", "**/*.config.mjs", "scripts/**/*.mjs"];

export default [
  ...svelteEslint.configs["flat/recommended"],
  globalIgnores(["dist/", "node_modules/", "src-tauri/", "eslint-rules/"]),
  {
    files: ["**/*.{ts,js,mjs}"],
    languageOptions: {
      parser: tsEslint.parser,
      globals: browserGlobals
    },
    plugins: sharedPlugins,
    rules: tsStyleRules
  },
  {
    files: nodeToolingFiles,
    languageOptions: {
      globals: globals.node
    }
  },
  {
    files: ["**/*.svelte"],
    languageOptions: {
      parser: svelteParser,
      parserOptions: {
        parser: tsEslint.parser,
        extraFileExtensions: [".svelte"]
      },
      globals: browserGlobals
    },
    plugins: sharedPlugins,
    rules: {
      ...tsStyleRules,
      "svelte/no-at-html-tags": "off",
      "svelte/indent": ["error", { indent: 2 }],
      "svelte/sort-attributes": "error",
      "svelte/shorthand-directive": "error",
      "svelte/first-attribute-linebreak": ["error"],
      "svelte/shorthand-attribute": ["error", { prefer: "always" }],
      "local/expand-svelte-block": "error",
      "prefer-const": ["error", { destructuring: "all" }]
    }
  }
];
