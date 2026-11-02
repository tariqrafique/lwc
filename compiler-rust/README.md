# lwc-compiler-rust
Full-Rust port of the LWC compiler.
- Output is ESM-only (AMD removed).
- Consumers move onto the Rust compiler (no JS facade kept long-term).
- Phase 0: behavioral harness comparing Rust output to the JS compiler.
- Style gate: lwc-style (cssparser-based) must match all 69 style-compiler fixtures byte-for-byte.
- Template: lwc-template ports @lwc/template-compiler; HTML via tariqrafique/parse5-rust; goldens are behavioral/AST/warnings (not astring bytes).
Crates: lwc-compiler (facade), lwc-style, lwc-template, lwc-ssr, lwc-js, lwc-shared.
