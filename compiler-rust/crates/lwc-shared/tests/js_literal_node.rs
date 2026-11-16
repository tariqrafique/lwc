//! `string_literal` against node's `JSON.stringify` over a generated corpus of UTF-16 strings,
//! including the D-0243 classes: lone surrogates, U+2028/U+2029, controls, quotes, and backslash.
//! Fails when node is missing.

use lwc_shared::js::string_literal;
use std::io::Write;
use std::process::{Command, Stdio};

/// The classes the quoter must match, named so a trim of the random corpus cannot drop them.
fn required_classes() -> Vec<Vec<u16>> {
    vec![
        // quotes and backslash
        vec![0x22],
        vec![0x5c],
        vec![0x22, 0x5c, 0x22],
        "a\"b\\c".encode_utf16().collect(),
        // controls: NUL, backspace, tab, LF, VT, FF, CR, other C0, DEL
        vec![0x00],
        vec![0x08],
        vec![0x09],
        vec![0x0a],
        vec![0x0b],
        vec![0x0c],
        vec![0x0d],
        vec![0x1f],
        vec![0x7f],
        // line and paragraph separators stay raw, matching JSON.stringify
        vec![0x2028],
        vec![0x2029],
        // lone surrogates, and a well-formed pair
        vec![0xd800],
        vec![0xdbff],
        vec![0xdc00],
        vec![0xdfff],
        vec![0xd800, 0x61, 0xdfff],
        vec![0xd800, 0xdc00],
        vec![0xd83d, 0xde00],
    ]
}

fn corpus() -> Vec<Vec<u16>> {
    let mut cases = required_classes();
    cases.extend((0..0x80).chain(0x7f..0xa1).map(|u| vec![u]));
    let specials: &[&[u16]] = &[
        &[0xfffd],
        &[0xfeff],
        &[0xdbff, 0xdfff],
        &[0xdc00, 0xd800],
        &[0xd800, 0xd800, 0xdc00],
        &[0xd800, 0x61],
        &[0x61, 0xdc00],
        &[0x00, 0x31],
        &[],
    ];
    cases.extend(specials.iter().map(|s| s.to_vec()));
    // Deterministic mixes over a palette that pairs and splits surrogates at random.
    let palette: [u16; 16] = [
        0x00, 0x08, 0x0a, 0x1f, 0x20, 0x22, 0x2e, 0x5c, 0x61, 0x7f, 0xe9, 0x2028, 0x2029, 0xd83d,
        0xde00, 0xdfff,
    ];
    let mut seed: u32 = 0x2545_f491;
    for _ in 0..500 {
        let mut s = Vec::new();
        for _ in 0..(seed % 12) {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            s.push(palette[(seed >> 16) as usize % palette.len()]);
        }
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        cases.push(s);
    }
    cases
}

fn node_json_stringify(cases: &[Vec<u16>]) -> Vec<String> {
    let input: String = cases
        .iter()
        .map(|c| c.iter().map(u16::to_string).collect::<Vec<_>>().join(",") + "\n")
        .collect();
    let script = "let d='';process.stdin.on('data',c=>d+=c).on('end',()=>{for(const l of d.split('\\n').slice(0,-1))\
        process.stdout.write(JSON.stringify(l?String.fromCharCode(...l.split(',').map(Number)):'')+'\\n')})";
    let mut node = Command::new("node")
        .arg("-e")
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("node is required");
    node.stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = node.wait_with_output().unwrap();
    assert!(out.status.success());
    std::str::from_utf8(&out.stdout)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn string_literal_required_classes_match_node_json_stringify() {
    let cases = required_classes();
    let expected = node_json_stringify(&cases);
    assert_eq!(expected.len(), cases.len());
    for (units, want) in cases.iter().zip(expected) {
        assert_eq!(string_literal(units.iter().copied()), want, "{units:x?}");
    }
}

#[test]
fn string_literal_matches_node_json_stringify() {
    let cases = corpus();
    let expected = node_json_stringify(&cases);
    assert_eq!(expected.len(), cases.len());
    for (units, want) in cases.iter().zip(expected) {
        assert_eq!(string_literal(units.iter().copied()), want, "{units:x?}");
    }
}
