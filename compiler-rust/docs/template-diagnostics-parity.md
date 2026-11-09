# Template compiler diagnostics parity (lwc-template vs @lwc/template-compiler b9e8007 / v9.5.3)

Scope: validation and diagnostics only (code, exact message, level, line:col). Codegen is out of scope.
Oracle: `/workspace/lwc` @ b9e8007, read-only. The fixture metadata comes from
`packages/@lwc/template-compiler/src/__tests__/fixtures/**/metadata.json`.

How to regenerate (scratch tools, read-only against the workspace):
- probe: `/workspace/scratch/diag-probe` runs `lwc_template::parse` over every upstream fixture and writes JSONL
- inventory: `node /workspace/scratch/diag-tools/inventory.mjs <probe.jsonl> --md out.md --json out.json`

## Summary (2026-11-01 20:13 PST)

| | baseline (19:56) | now |
|---|---|---|
| fixtures with every diagnostic exact (567 total) | 327 | 438 |
| codes matched | 0 | 29 |
| codes mismatched | 29 | 10 |
| codes missing (upstream emits, Rust never does) | 86 | 76 |
| not-emitted-untested / unused-upstream | 6 / 11 | 6 / 11 |

Board, official runs via `harness/board.sh`, test counts:

| gate | napi 6f2f288a4bbf | napi a15a9e885974 |
|---|---|---|
| csr-render-template | 175/570 | 266/570 |
| csr-render | 190/390 | 190/390 |

### What changed
- All coded diagnostics now go through the lwc-shared `ParserDiagnostics` / `TemplateErrors` entries (`X::NAME.generate(args, loc)`) → `CompilerDiagnostic::generate`, so they carry the `LWCnnnn: ` prefix and the upstream text.
- `expr_check.rs` is a faithful port of both upstream expression validators:
  - legacy `parseExpression`: validateSourceIsParsedExpression (1074/1083), validateExpression (1060/1038/1207-1210)
  - CTE `validateExpressionAst` (1083 acorn-style `(line:col)`, 1190-1197, 1192)
  - it is gated on apiVersion >= 66, honours quotedness, and uses the full text node as the CTE source
- 1034 (ambiguous quoted attribute) is emitted where quotedness can be proven from source length.

### Known gaps needing other owners
- html-decoding fixtures (4) need the raw text and attribute source (lowering, parser/mod.rs).
- `"{x}"` on the source-last attribute: parse5-rust stretches that attribute's end over trailing whitespace and `/`, so quotedness is ambiguous there (parse5-rust tokenizer).
- 1165 column uses the attribute location instead of the element location (parser/mod.rs).
- Warning `(line:col)` in the rollup/napi path is a napi JsDiagnostic location item, owned by Harness.

## Inventory

| code | name | level | upstream-used | Rust emits | status | fixtures | expected diags | found | msg ok* | level ok | loc ok | exact | extra | sample gap |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1003 | INVALID_TEMPLATE | Error | n | n | unused-upstream | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1028 | OPTIONS_MUST_BE_OBJECT | Error | n | n | unused-upstream | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1029 | UNKNOWN_IF_MODIFIER | Error | n | n | unused-upstream | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1030 | UNKNOWN_OPTION_PROPERTY | Error | n | n | unused-upstream | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1034 | AMBIGUOUS_ATTRIBUTE_VALUE | Error | y | n | missing | 2 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | attributes/error-ambiguous-attribute: missing |
| 1035 | AMBIGUOUS_ATTRIBUTE_VALUE_STRING | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | attributes/error-ambiguous-attribute: missing |
| 1036 | BOOLEAN_ATTRIBUTE_FALSE | Error | y | n | missing | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | attributes/error-boolean: missing |
| 1037 | BOOLEAN_ATTRIBUTE_TRUE | Error | y | n | missing | 2 | 18 | 0 | 0 | 0 | 0 | 0 | 0 | attributes/boolean-attributes-invalid: missing |
| 1038 | COMPUTED_PROPERTY_ACCESS_NOT_ALLOWED | Error | y | n | matched | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 0 |  |
| 1039 | DIRECTIVE_SHOULD_BE_EXPRESSION | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-iterator/error-invalid-expression: missing |
| 1040 | INVALID_ID_ATTRIBUTE | Error | y | n | missing | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | parsing-errors/invalid-id: missing |
| 1041 | INVALID_STATIC_ID_IN_ITERATION | Warning | y | n | matched | 2 | 5 | 5 | 5 | 5 | 5 | 5 | 0 |  |
| 1042 | DUPLICATE_ID_FOUND | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | parsing-errors/invalid-id: missing |
| 1043 | EVENT_HANDLER_SHOULD_BE_EXPRESSION | Error | y | n | missing | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | events/error-component-handler-name: missing |
| 1044 | FOR_EACH_AND_FOR_ITEM_DIRECTIVES_SHOULD_BE_TOGETHER | Error | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | directive-for-each/error-invalid-for-each-only: missing |
| 1045 | FOR_EACH_DIRECTIVE_SHOULD_BE_EXPRESSION | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-for-each/error-invalid-type: missing |
| 1046 | FOR_INDEX_DIRECTIVE_SHOULD_BE_STRING | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-for-each/error-invalid-type: missing |
| 1047 | FOR_ITEM_DIRECTIVE_SHOULD_BE_STRING | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-for-each/error-invalid-type: missing |
| 1048 | FORBIDDEN_IFRAME_SRCDOC_ATTRIBUTE | Error | y | n | missing | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | parsing-errors/forbidden-iframe-srcdoc-attribute: missing |
| 1049 | FORBIDDEN_SVG_NAMESPACE_IN_TEMPLATE | Error | y | n | missing | 4 | 18 | 0 | 0 | 0 | 0 | 0 | 0 | html-tags/blacklisted-svg: missing |
| 1050 | FORBIDDEN_MATHML_NAMESPACE_IN_TEMPLATE | Error | y | n | missing | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | parsing-errors/forbidden-base-tag: missing |
| 1051 | FORBIDDEN_TAG_ON_TEMPLATE | Error | y | n | mismatched | 5 | 6 | 6 | 6 | 6 | 6 | 6 | 1 | parsing-errors/title-no-close-tag: extra "LWC1051: Forbidden tag found in template: '<title>' tag is not allowed." |
| 1052 | GENERIC_PARSING_ERROR | Error | y | n | not-emitted-untested | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1053 | IDENTIFIER_PARSING_ERROR | Error | n | n | unused-upstream | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1054 | IF_DIRECTIVE_SHOULD_BE_EXPRESSION | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-if/error-invalid-expression: missing |
| 1055 | INVALID_ATTRIBUTE_CASE | Error | y | n | missing | 3 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | attributes/error-camelcase: missing |
| 1056 | INVALID_EVENT_NAME | Error | y | n | missing | 1 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | events/type-invalid: missing |
| 1057 | INVALID_HTML_ATTRIBUTE | Warning | y | n | mismatched | 9 | 23 | 23 | 23 | 23 | 22 | 22 | 25 | attributes/scoped-id/expression/scoped: extra "LWC1057: for is not valid attribute for div. For more information refer to https://developer.mozill |
| 1058 | INVALID_HTML_SYNTAX | Error | y | n | matched | 11 | 12 | 12 | 12 | 12 | 12 | 12 | 0 |  |
| 1059 | INVALID_IDENTIFIER | Error | y | n | missing | 3 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | directive-for-each/error-invalid-identifier: missing |
| 1060 | INVALID_NODE | Error | y | n | matched | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 0 |  |
| 1061 | INVALID_TABINDEX_ATTRIBUTE | Error | y | n | missing | 1 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | attributes/tabindex: missing |
| 1062 | DEPRECATED_IS_ATTRIBUTE_CANNOT_BE_EXPRESSION | Error | n | n | unused-upstream | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1063 | IS_ATTRIBUTE_NOT_SUPPORTED | Error | y | n | missing | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | attributes/error-is: missing |
| 1064 | KEY_ATTRIBUTE_SHOULD_BE_EXPRESSION | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-key/error-expression: missing |
| 1065 | KEY_SHOULDNT_REFERENCE_FOR_EACH_INDEX | Error | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | directive-key/error-for-each-index-as-key: missing |
| 1066 | KEY_SHOULDNT_REFERENCE_ITERATOR_INDEX | Error | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | directive-key/error-inline-iterator-index-as-key: missing |
| 1071 | MISSING_KEY_IN_ITERATOR | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-for-each/error-missing-key: missing |
| 1072 | MISSING_ROOT_TEMPLATE_TAG | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | parsing-errors/empty-template: missing |
| 1073 | MODIFYING_ITERATORS_NOT_ALLOWED | Error | n | n | unused-upstream | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1074 | MULTIPLE_EXPRESSIONS | Error | y | n | matched | 2 | 3 | 3 | 3 | 3 | 3 | 3 | 0 |  |
| 1075 | MULTIPLE_ROOTS_FOUND | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | parsing-errors/multiple-root: missing |
| 1076 | NAME_ON_SLOT_CANNOT_BE_EXPRESSION | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | slots/error-definition-dynamic-name: missing |
| 1077 | NO_DIRECTIVE_FOUND_ON_TEMPLATE | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | parsing-errors/invalid-template-element: missing |
| 1078 | NO_MATCHING_CLOSING_TAGS | Error | y | n | missing | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | parsing-errors/no-closing-tag: missing |
| 1079 | ROOT_TAG_SHOULD_BE_TEMPLATE | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | parsing-errors/invalid-root-tag: missing |
| 1080 | ROOT_TEMPLATE_HAS_UNKNOWN_ATTRIBUTES | Error | y | n | matched | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 0 |  |
| 1082 | SLOT_TAG_CANNOT_HAVE_DIRECTIVES | Error | y | n | missing | 2 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | directive-lwc-if-else/slots/duplicate-slots-warning/no-directives-in-slots: missing |
| 1083 | TEMPLATE_EXPRESSION_PARSING_ERROR | Error | y | n | mismatched | 13 | 15 | 11 | 11 | 11 | 11 | 11 | 0 | expression-complex/invalid/html-decoding/experimental-complex-expressions-true: missing |
| 1084 | UNEXPECTED_IF_MODIFIER | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-if/error-invalid-modifier: missing |
| 1085 | LWC_DOM_INVALID_VALUE | Error | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | lwc-dom/error-expression: missing |
| 1086 | LWC_DOM_INVALID_CONTENTS | Error | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | lwc-dom/error-comments-preserve-comments: missing |
| 1087 | LWC_DOM_INVALID_CUSTOM_ELEMENT | Error | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | lwc-dom/error-custom-element: missing |
| 1088 | LWC_DOM_INVALID_SLOT_ELEMENT | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | lwc-dom/error-slot-element: missing |
| 1122 | STYLE_TAG_NOT_ALLOWED_IN_TEMPLATE | Error | y | n | missing | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | parsing-errors/forbidden-style-tag: missing |
| 1123 | UNKNOWN_HTML_TAG_IN_TEMPLATE | Warning | y | n | mismatched | 7 | 12 | 12 | 12 | 12 | 12 | 12 | 9 | parsing-errors/multiple-root: extra "LWC1123: Unknown html tag '<root>'. For more information refer to https://developer.mozilla.org/en- |
| 1124 | ATTRIBUTE_NAME_STARTS_WITH_INVALID_CHARACTER | Error | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | attributes/invalid-error-underscore-attribute: missing |
| 1125 | ATTRIBUTE_NAME_MUST_END_WITH_ALPHA_NUMERIC_CHARACTER | Error | y | n | missing | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | attributes/invalid-error-underscore-attribute: missing |
| 1127 | UNKNOWN_LWC_DIRECTIVE | Error | y | n | matched | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 0 |  |
| 1128 | INVALID_OPTS_LWC_DYNAMIC | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-dynamic/invalid-config: missing |
| 1129 | INVALID_LWC_DYNAMIC_ON_NATIVE_ELEMENT | Error | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | directive-dynamic/invalid-native-element: missing |
| 1130 | INVALID_LWC_DYNAMIC_LITERAL_PROP | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-dynamic/invalid-type: missing |
| 1133 | LWC_RENDER_MODE_INVALID_VALUE | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-render-mode/error-invalid-value: missing |
| 1134 | LWC_LIGHT_SLOT_INVALID_ATTRIBUTES | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-render-mode/error-slots-attributes: missing |
| 1135 | LWC_DOM_INVALID_IN_LIGHT_DOM | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-render-mode/error-dom-manual: missing |
| 1136 | INVALID_FOR_EACH_WITH_ITERATOR | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-iterator/error-for-each-mix: missing |
| 1137 | NO_DUPLICATE_SLOTS | Warning | y | n | matched | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 0 |  |
| 1138 | NO_SLOTS_IN_ITERATOR | Warning | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | parsing-errors/named-slot-in-iterator: missing |
| 1139 | LWC_LIGHT_SLOT_INVALID_EVENT_LISTENER | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-render-mode/error-slots-event-listeners: missing |
| 1140 | LWC_INNER_HTML_INVALID_CUSTOM_ELEMENT | Error | y | n | missing | 2 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | directive-inner-html/invalid: missing |
| 1141 | LWC_INNER_HTML_INVALID_ELEMENT | Error | y | n | missing | 2 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | directive-inner-html/invalid: missing |
| 1142 | LWC_INNER_HTML_INVALID_CONTENTS | Error | y | n | missing | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | directive-inner-html/invalid: missing |
| 1143 | LWC_INNER_HTML_INVALID_VALUE | Error | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | directive-inner-html/invalid: missing |
| 1144 | INVALID_HTML_RECOVERY | Warning | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | regression/invalid-html-recovery: missing |
| 1145 | INVALID_TEMPLATE_ATTRIBUTE | Warning | y | n | mismatched | 5 | 6 | 6 | 6 | 6 | 6 | 6 | 5 | directive-external/template: extra "LWC1145: Invalid attributes detected on template. The following attributes are not supported on tem |
| 1146 | PRESERVE_COMMENTS_MUST_BE_BOOLEAN | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | comments/error-invalid-type: missing |
| 1147 | DUPLICATE_ATTR_PROP_TRANSFORM | Warning | y | n | missing | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | attributes/duplicate-props: missing |
| 1148 | INVALID_HTML_SYNTAX_WARNING | Warning | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | parsing-errors/double-close-template-older-api-version: missing |
| 1149 | KEY_SHOULD_BE_IN_ITERATION | Warning | y | n | mismatched | 3 | 6 | 6 | 6 | 6 | 6 | 6 | 3 | directive-key/nested-iterator-if: extra "LWC1149: Invalid key attribute on element <div>. The key attribute should be applied to an element  |
| 1150 | DUPLICATE_ELEMENT_ENTRY | Error | n | n | unused-upstream | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1151 | CUSTOM_ELEMENT_TAG_DISALLOWED | Error | n | n | unused-upstream | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1153 | INVALID_TEMPLATE_WARNING | Warning | y | n | mismatched | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 10 | directive-for-each/error-invalid-for-each-only: extra "LWC1153: Non-root template elements must contain valid LWC template directive attributes. Otherwise |
| 1155 | INVALID_LWC_SPREAD_LITERAL_PROP | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-spread/invalid-not-expression: missing |
| 1156 | LWC_REF_INVALID_ELEMENT | Error | y | n | missing | 1 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | directive-ref/invalid: missing |
| 1157 | LWC_REF_INVALID_VALUE | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-ref/invalid: missing |
| 1158 | LWC_REF_INVALID_LOCATION_INSIDE_ITERATION | Error | y | n | missing | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | directive-ref/invalid: missing |
| 1159 | IF_BLOCK_DIRECTIVE_SHOULD_BE_EXPRESSION | Error | y | n | not-emitted-untested | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1160 | ELSEIF_BLOCK_DIRECTIVE_SHOULD_BE_EXPRESSION | Error | y | n | not-emitted-untested | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1161 | ELSE_BLOCK_DIRECTIVE_CANNOT_HAVE_VALUE | Error | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | directive-lwc-if-else/error-invalid-expression: missing |
| 1162 | INVALID_IF_BLOCK_DIRECTIVE_WITH_CONDITIONAL | Error | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | directive-lwc-if-else/error-multiple-directives/if-else: missing |
| 1163 | INVALID_ELSEIF_BLOCK_DIRECTIVE_WITH_CONDITIONAL | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-lwc-if-else/error-multiple-directives/elseif-else: missing |
| 1164 | INVALID_ELSE_BLOCK_DIRECTIVE_WITH_CONDITIONAL | Error | n | n | unused-upstream | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1165 | LWC_IF_SCOPE_NOT_FOUND | Error | y | n | mismatched | 7 | 7 | 7 | 7 | 7 | 0 | 0 | 1 | directive-lwc-if-else/error-invalid-sibling/else/cannot-be-preceeded-by-text-node:   loc 4:15 want 4:5 |
| 1166 | LWC_IF_CANNOT_BE_USED_WITH_IF_DIRECTIVE | Error | y | n | missing | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | directive-lwc-if-else/interop/mixed-if-true-else: missing |
| 1169 | SCOPED_SLOT_BIND_IN_LIGHT_DOM_ONLY | Error | y | n | matched | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0 |  |
| 1170 | INVALID_LWC_SLOT_BIND_LITERAL_PROP | Error | y | n | matched | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0 |  |
| 1171 | INVALID_LWC_SLOT_BIND_NON_SLOT_ELEMENT | Error | y | n | matched | 1 | 4 | 4 | 4 | 4 | 4 | 4 | 0 |  |
| 1172 | NO_DUPLICATE_SCOPED_SLOT | Error | y | n | matched | 5 | 6 | 6 | 6 | 6 | 6 | 6 | 0 |  |
| 1173 | NO_MIXED_SLOT_TYPES | Error | y | n | matched | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 0 |  |
| 1174 | SLOT_DATA_VALUE_SHOULD_BE_STRING | Error | y | n | matched | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0 |  |
| 1176 | SCOPED_SLOT_DATA_ON_TEMPLATE_ONLY | Error | y | n | matched | 1 | 5 | 5 | 5 | 5 | 5 | 5 | 0 |  |
| 1177 | NON_ELEMENT_SCOPED_SLOT_CONTENT | Error | y | n | matched | 4 | 8 | 8 | 8 | 8 | 8 | 8 | 0 |  |
| 1178 | INVALID_PARENT_OF_LWC_SLOT_DATA | Error | y | n | matched | 1 | 2 | 2 | 2 | 2 | 2 | 2 | 0 |  |
| 1179 | SCOPED_SLOTDATA_CANNOT_BE_COMBINED_WITH_OTHER_DIRECTIVE | Error | y | n | matched | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 0 |  |
| 1180 | INVALID_LWC_EXTERNAL_ON_NON_CUSTOM_ELEMENT | Error | y | n | missing | 3 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | directive-external/built-ins: missing |
| 1181 | INVALID_LWC_EXTERNAL_VALUE | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-external/non-boolean-usage: missing |
| 1182 | SINGLE_IF_DIRECTIVE_ALLOWED | Warning | y | n | matched | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 0 |  |
| 1183 | LWC_COMPONENT_TAG_WITHOUT_IS_DIRECTIVE | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | dynamic-components/invalid/missing-is-directive: missing |
| 1184 | UNSUPPORTED_LWC_TAG_NAME | Warning | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | lwc-tags/invalid: missing |
| 1185 | INVALID_LWC_IS_DIRECTIVE_VALUE | Error | y | n | missing | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | dynamic-components/invalid/invalid-is-directive-value: missing |
| 1186 | LWC_IS_INVALID_ELEMENT | Error | y | n | missing | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | dynamic-components/invalid/invalid-is-directive-usage/elements: missing |
| 1187 | DEPRECATED_LWC_DYNAMIC_ATTRIBUTE | Warning | y | n | mismatched | 12 | 12 | 12 | 12 | 12 | 12 | 12 | 1 | scoped-slots/invalid/slot-data/invalid-on-non-slot-content: extra "LWC1187: The lwc:dynamic directive is deprecated and will be removed in a future release. Please us |
| 1188 | INVALID_OPTS_LWC_ENABLE_DYNAMIC_COMPONENTS | Error | y | n | missing | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | dynamic-components/invalid/invalid-config: missing |
| 1189 | INVALID_EXPR_ARROW_FN_PARAM | Error | y | n | not-emitted-untested | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1190 | INVALID_EXPR_STATEMENTS_PROHIBITED | Error | y | n | not-emitted-untested | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1191 | INVALID_EXPR_MUTATION_OUTSIDE_ARROW | Error | y | n | matched | 3 | 3 | 3 | 3 | 3 | 3 | 3 | 0 |  |
| 1192 | INVALID_EXPR_DELETE_OP | Error | y | n | matched | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0 |  |
| 1193 | INVALID_EXPR_ARROW_FN_BODY | Error | y | n | matched | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0 |  |
| 1194 | INVALID_EXPR_ARROW_FN_KIND | Error | y | n | matched | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0 |  |
| 1195 | INVALID_EXPR_EARLY_STAGE_FEATURE | Error | n | n | unused-upstream | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1196 | INVALID_EXPR_PROHIBITED_NODE_TYPE | Error | y | n | matched | 9 | 9 | 9 | 9 | 9 | 9 | 9 | 0 |  |
| 1197 | INVALID_EXPR_COMMENTS_DISALLOWED | Error | y | n | matched | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0 |  |
| 1201 | IGNORED_SLOT_ATTRIBUTE_IN_CHILD | Warning | y | n | mismatched | 4 | 4 | 4 | 4 | 4 | 4 | 4 | 2 | slots/dynamic-components/usage: extra "LWC1201: The slot attribute in <p> will be ignored due to its ancestor <lwc:component>. It must be  |
| 1203 | INVALID_LWC_ON_ELEMENT | Error | y | n | not-emitted-untested | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 1204 | INVALID_LWC_ON_LITERAL_PROP | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-on/invalid-not-expression: missing |
| 1205 | INVALID_LWC_ON_WITH_DECLARATIVE_LISTENERS | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-on/invalid-declarative-listeners: missing |
| 1206 | INVALID_LWC_ON_OPTS | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | directive-on/invalid-disabled: missing |
| 1207 | COMPUTED_PROPERTY_ACCESS_NOT_ALLOWED_CTE_UNQUOTED | Error | y | n | matched | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0 |  |
| 1208 | INVALID_NODE_CTE_UNQUOTED | Error | y | n | matched | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0 |  |
| 1209 | COMPUTED_PROPERTY_ACCESS_NOT_ALLOWED_CTE_API_VERSION | Error | y | n | matched | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 0 |  |
| 1210 | INVALID_NODE_CTE_API_VERSION | Error | y | n | matched | 2 | 2 | 2 | 2 | 2 | 2 | 2 | 0 |  |
| 1211 | RESERVED_KEYWORD_AS_IDENTIFIER | Error | y | n | missing | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | expression/reverved-keyword: missing |

## Status 2026-11-01 20:51 PST (post-reboot reconciliation)

The table above is the earlier generated snapshot; per-code rows are stale.
Live probe (code, message, level, line:col exact per fixture): 560/567 (verified-5e72713 tree: 547; pre-restore live: 542).
Board at napi ed21d7eba9e4: csr-render-template 406/570, csr-render 243/390 (tree also carries Template Rust in-flight work).

Remaining non-exact fixtures:
- mixed-if-true-else: missing 1166 (element_check).
- no-directives-in-slots: 1165 instead of 1082 (element_check order).
- blacklisted-svg: 1057 warnings truncated by the childless rollback before element_check's 1049.
- empty-template: missing 1072 (mod.rs).
- invalid-html-recovery, table-with-tr: 1144 needs the `validate::html_recovery(n, d)` call in mod.rs; table-with-tr also has an extra 1078 for the implied tbody (element_check should validate the fallback parent location).
- slots/dynamic-components/usage: extra 1201; mod.rs slot_ancestor must treat `<lwc:component>` as transparent.

## Status 2026-11-01 20:59 PST
- Probe 566/567 exact. Remaining miss: directive-lwc-if-else/error-multiple-directives/if-else (extra 1165 from mod.rs orphan_else post-pass after element_check's 1162 throw; Template Rust).
- 1043/1142/1180/1036/1037/1055/1124/1125/1049/1050/1054/1071/1149: all fixtures exact (element_check / mod.rs / validate::key_in_iteration). No port needed in validate.rs.
- 1190 unreachable (parent arrow/class/function node throws 1193/1196 first); 1189 is codegen; 1052 is the index.ts catch-all.
- 1057/1184 now emitted only by element_check via validate::{attr_name_warnings, unknown_lwc_tag}; validate::element no longer calls them.
- csr-render-template 449/570 @ napi f456a3353b1f (20:57:56 PST): no diagnostic mismatches among failures; 39 status mismatches are codegen (iteration over undefined -> "is not a vnode"; member access on undefined not throwing).
