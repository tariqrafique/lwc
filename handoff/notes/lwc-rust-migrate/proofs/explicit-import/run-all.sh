#!/bin/bash
# Regenerates shipped/Rust outputs for each input and runs the ESM eval. Requires /workspace/oracle953 and the
# lwc-js `opts` example.
set -e
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"
for v in input input-reassign input-anon; do
  node -e "
    const fs=require('fs');const {transformSync}=require('module').createRequire('/workspace/oracle953/')('@lwc/compiler');
    fs.writeFileSync('out-shipped-$v.js', transformSync(fs.readFileSync('$v.js','utf8'),'foo.js',{name:'foo',namespace:'x',isExplicitImport:true}).code);"
  (cd /workspace/lwc-compiler-rust && cargo run -q -p lwc-js --example opts -- "$OLDPWD/$v.js" explicit) > "out-rust-$v.js"
  echo "== $v"; (cd eval && node eval.mjs "../out-shipped-$v.js" "../out-rust-$v.js" > "../eval-$v.json" 2>&1 && tail -1 "../eval-$v.json") || { echo "DIFFERENT (see eval-$v.json)"; }
done
