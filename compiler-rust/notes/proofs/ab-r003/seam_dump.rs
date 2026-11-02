// Temporary napi unit-test module (included via #[path], never committed): dumps the napi `transform` result for
// every case in $SEAM_CORPUS to $SEAM_OUT, so the AB-R003 refactor can be diffed byte-for-byte at the seam.
use serde_json::{Value, json};
use crate::*;

fn loc(l: &JsDiagnosticLocation) -> Value { json!({"line": l.line, "column": l.column, "start": l.start, "length": l.length, "index": l.index}) }
fn diag(d: &JsDiagnostic) -> Value { json!({"code": d.code, "level": d.level, "message": d.message, "filename": d.filename, "location": d.location.as_ref().map(loc)}) }

#[test]
#[ignore]
fn seam_dump() {
    let corpus: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(std::env::var("SEAM_CORPUS").unwrap()).unwrap()).unwrap();
    let mut out = vec![];
    for c in corpus {
        let r = std::panic::catch_unwind(|| transform(c["src"].as_str().unwrap().into(), c["filename"].as_str().unwrap().into(), c["opts"].clone()));
        let v = match r {
            Err(_) => json!("PANIC"),
            Ok(Err(e)) => json!({"thrown": e.reason}),
            Ok(Ok(r)) => match &r.error {
                None => json!({"code": r.code, "map": r.map, "tokens": r.css_scope_tokens, "warnings": r.warnings.as_ref().map(|w| w.iter().map(diag).collect::<Vec<_>>())}),
                Some(e) => json!({
                    "ownKeys": e.own_keys(), "message": e.message, "name": e.name, "code": e.code, "line": e.line, "column": e.column,
                    "filename": e.filename, "level": e.level, "url": e.url,
                    "location": match &e.location { None => Value::Null, Some(napi::Either::A(l)) => loc(l), Some(napi::Either::B(())) => json!("undefined") },
                    "errors": e.errors.as_ref().map(|es| es.iter().map(|x| json!({"name": x.name, "message": x.message, "code": x.code, "filename": x.filename, "location": x.location.as_ref().map(loc), "level": x.level, "url": x.url})).collect::<Vec<_>>()),
                }),
            },
        };
        out.push(v);
    }
    std::fs::write(std::env::var("SEAM_OUT").unwrap(), serde_json::to_string_pretty(&out).unwrap()).unwrap();
}
