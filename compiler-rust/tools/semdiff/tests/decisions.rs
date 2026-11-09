//! SD-1A loader validation: case_key shape, the registry (coverage-gate id rules), and the case_key → D-id map.
//! Every rule has a negative (fires) and the nearest positive (silent).
use semdiff::decisions::{check_case_key, DecisionMap, FindingCode, Registry, DECISIONS_MAP_TSV};
use semdiff::{bucket, sha256_hex, summarize, Case, Class, Manifest};

const H: &str = "id\tts\tphase\tdecision\twhy\tevidence\tresult";

fn key(corpus: &str, source: &str) -> String {
    format!("{corpus}/{source}/{}/{}", sha256_hex(source.as_bytes()), sha256_hex(b"opts"))
}

fn reg() -> Registry {
    Registry::parse(&format!(
        "{H}\nD-0001\tt\tp\td\tw\te\tlocked\nD-0002\tt\tp\td\tw\te\topen\nD-0003\tt\tp\td\tw\nD-0196\tt\tp\td\tw\te\tlocked\n"
    ))
    .unwrap()
}

fn map(rows: &str) -> Result<DecisionMap, String> {
    DecisionMap::parse(&format!("case_key\tdecision_id\twhy\n{rows}"), &reg())
}

#[test]
fn case_key_shape() {
    let h = sha256_hex(b"x");
    assert_eq!(check_case_key(&key("tdd", "fixtures/a/actual.html")), Ok("tdd"));
    assert_eq!(check_case_key(&format!("c.1_x-y/s/{h}/{h}")), Ok("c.1_x-y"));
    for bad in [
        "eq".to_string(),                                   // index-style id
        format!("tdd/{h}/{h}"),                             // no source
        format!("tdd//{h}/{h}"),                            // empty source
        format!("tdd/a//b/{h}/{h}"),                        // empty source segment
        format!("tdd/a/../b/{h}/{h}"),                      // dot-dot segment
        format!("tdd/a b/{h}/{h}"),                         // whitespace (not one TSV field)
        format!("tdd/a\tb/{h}/{h}"),                        // tab
        format!("tdd/a/{}/{h}", h.to_uppercase()),          // uppercase hex
        format!("tdd/a/{}/{h}", &h[1..]),                   // 63 hex
        format!("t:d/a/{h}/{h}"),                           // bad corpus char
    ] {
        assert!(check_case_key(&bad).is_err(), "{bad:?} must be rejected");
    }
}

#[test]
fn registry_rules() {
    assert!(reg().require_locked("D-0001").is_ok());
    assert!(reg().require_locked("D-0196").is_ok());
    let e = |t: &str| Registry::parse(t).err().unwrap_or_default();
    // No id column: the file is usable but nothing resolves (coverage-gate-spec: DEC-UNRESOLVED on every cite).
    let r = Registry::parse("ts\tresult\nx\tlocked\n").unwrap();
    assert!(!r.has_id_column && r.is_empty());
    assert!(r.require_locked("D-0001").unwrap_err().contains("DEC-UNRESOLVED: D-0001: the registry has no `id` column"));
    // No result column: fail closed, every row is not locked.
    let r = Registry::parse("id\tts\nD-0001\tx\n").unwrap();
    assert_eq!((r.len(), r.locked()), (1, 0));
    assert!(r.require_locked("D-0001").unwrap_err().contains("DEC-NOT-LOCKED"));
    // MALFORMED: empty file, or a header naming a column twice.
    assert!(e("").contains("MALFORMED"));
    assert!(e("\nD-0001\n").contains("MALFORMED"));
    assert!(e("id\tresult\tid\nD-0001\tlocked\tD-0002\n").contains("MALFORMED (line 1): header names column \"id\" twice"));
    // check() reports every defect, in line order, not just the first.
    let (_, f) = Registry::check(&format!("{H}\nD-1\tt\nD-0001\tt\nD-0001\tt\n"));
    let got: Vec<_> = f.iter().map(|f| (f.code, f.line, f.id.clone().unwrap())).collect();
    assert_eq!(got, [(FindingCode::DecIdInvalid, Some(2), "D-1".into()), (FindingCode::DecIdInvalid, Some(4), "D-0001".into())]);
    assert!(f.iter().all(|f| f.code.is_defect()));
    let (r, f) = Registry::check(&format!("{H}\nD-0001\tt\tp\td\tw\te\tlocked\n"));
    assert!(f.is_empty() && r.resolve("D-0001").is_none());
    assert_eq!(r.resolve("D-0002").unwrap().code, FindingCode::DecUnresolved);
    assert!(!FindingCode::DecUnresolved.is_defect() && !FindingCode::DecNotLocked.is_defect());
    assert!(e(&format!("{H}\nD-001\tt\tp\td\tw\te\tlocked\n")).contains("DEC-ID-INVALID"), "3 digits");
    assert!(e(&format!("{H}\nD-00011\tt\tp\td\tw\te\tlocked\n")).contains("DEC-ID-INVALID"), "5 digits");
    assert!(e(&format!("{H}\nd-0001\tt\tp\td\tw\te\tlocked\n")).contains("DEC-ID-INVALID"), "case");
    assert!(e(&format!("{H}\n\n")).contains("DEC-ID-INVALID"), "blank row = missing id");
    assert!(e(&format!("{H}\nD-0001\ta\nD-0001\tb\n")).contains("duplicate id D-0001"), "union-merge duplicate");
    // An invalid id fails the whole registry even though nothing cites it (coverage gate: "whether or not any cell cites it").
    assert!(e(&format!("{H}\nD-0196\tt\tp\td\tw\te\tlocked\nX-1\tt\n")).contains("DEC-ID-INVALID"));
    // Resolution is exact-match only; a short legacy row (no result field) is simply not locked.
    assert!(reg().require_locked("D-01").unwrap_err().contains("DEC-UNRESOLVED"));
    assert!(reg().require_locked("D-0002").unwrap_err().contains("DEC-NOT-LOCKED"));
    assert!(reg().require_locked("D-0003").unwrap_err().contains("DEC-NOT-LOCKED"));
}

#[test]
fn map_rules() {
    let k = key("tdd", "a");
    let k2 = key("tdd", "b");
    let m = map(&format!("{k}\tD-0001\twhy\n{k2}\tD-0001\twhy too\n")).unwrap();
    assert_eq!((m.len(), m.get(&k), m.get(&key("tdd", "c"))), (2, Some("D-0001"), None));
    let e = |rows: String| map(&rows).err().unwrap_or_default();
    assert!(e(format!("{k}\tD-0001\n")).contains("BAD-LINE"));
    assert!(e(format!("{k}\tD-0001\twhy\textra\n")).contains("BAD-LINE"));
    assert!(e("eq\tD-0001\twhy\n".into()).contains("BAD-KEY"));
    assert!(e(format!("{k}\tD-1\twhy\n")).contains("DEC-ID-INVALID"));
    assert!(e(format!("{k}\tD-0001\t  \n")).contains("NO-EVIDENCE"));
    assert!(e(format!("{k}\tD-0009\twhy\n")).contains("DEC-UNRESOLVED"));
    assert!(e(format!("{k}\tD-0002\twhy\n")).contains("DEC-NOT-LOCKED"));
    assert!(e(format!("{k}\tD-0003\twhy\n")).contains("DEC-NOT-LOCKED"));
    assert!(e(format!("{k}\tD-0001\twhy\n{k}\tD-0196\tother\n")).contains("DUP-KEY"), "dup key, even with a different id");
    let hdr = DecisionMap::parse("case_key\tdecision\twhy\n", &reg()).err().unwrap();
    assert!(hdr.contains("BAD-HEADER"));
    assert!(DecisionMap::parse("", &reg()).err().unwrap().contains("BAD-HEADER"));
}

#[test]
fn shipped_map_is_valid() {
    // The compiled-in map must load against a registry where D-0196 is locked (it is empty in SD-1A).
    let m = DecisionMap::parse(DECISIONS_MAP_TSV, &reg()).unwrap();
    assert!(m.is_empty());
}

#[test]
fn bucket_consults_the_map_only_for_different() {
    let k = key("tdd", "a");
    let m = map(&format!("{k}\tD-0001\twhy\n")).unwrap();
    assert_eq!(bucket(Class::Different, &k, &m), ("different_decided", Some("D-0001".into())));
    assert_eq!(bucket(Class::Different, &key("tdd", "b"), &m), ("different_undecided", None));
    // A mapped key that is not DIFFERENT keeps its own bucket and gets no id: rewrite-unequal is not DIFFERENT.
    assert_eq!(bucket(Class::Unclassified, &k, &m), ("unclassified", None));
    assert_eq!(bucket(Class::EquivalentByRewrite, &k, &m), ("equivalent", None));
    assert_eq!(bucket(Class::Crash, &k, &m), ("crash", None));
}

#[test]
fn summary_reports_mapped_keys_that_are_not_different() {
    let (ka, kb) = (key("tdd", "a"), key("tdd", "b"));
    let m = map(&format!("{ka}\tD-0001\twhy\n")).unwrap();
    let man = Manifest {
        corpus: "tdd".into(),
        sha256: "x".into(),
        cases: vec![
            Case { id: ka.clone(), oracle: "f(1);".into(), rust: "f(2);".into() },
            Case { id: kb.clone(), oracle: "f(1);".into(), rust: "f(1);".into() },
        ],
    };
    let s = summarize(&man, &m, "r").unwrap();
    assert_eq!(s["schema"], "semdiff-summary/v2");
    assert_eq!(s["cases"][0]["bucket"], "unclassified");
    assert!(s["cases"][0]["decision_id"].is_null(), "rewrite-unequal never becomes different_decided");
    assert_eq!(s["decisions"]["applied"], 0);
    assert_eq!(s["decisions"]["entries"], 1);
    assert_eq!(s["decisions"]["mapped_not_different"], serde_json::json!([ka]));
    assert_eq!(s["decisions"]["registry_sha256"], "r");
    assert_eq!(s["totals"]["different_decided"], 0);
}
