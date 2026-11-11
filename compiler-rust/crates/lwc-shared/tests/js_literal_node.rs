//! `json_string` against node's `JSON.stringify` over a generated corpus of UTF-16 strings. Fails
//! when node is missing.

use lwc_shared::js_literal::json_string;
use std::io::Write;
use std::process::{Command, Stdio};

fn corpus() -> Vec<Vec<u16>> {
    let mut cases: Vec<Vec<u16>> = (0..0x80).chain(0x7f..0xa1).map(|u| vec![u]).collect();
    let specials: &[&[u16]] = &[
        &[0x2028],
        &[0x2029],
        &[0xfffd],
        &[0xfeff],
        &[0xd83d, 0xde00],
        &[0xd800, 0xdc00],
        &[0xdbff, 0xdfff],
        &[0xd800],
        &[0xdbff],
        &[0xdc00],
        &[0xdfff],
        &[0xdc00, 0xd800],
        &[0xd800, 0xd800, 0xdc00],
        &[0xd800, 0x61],
        &[0x61, 0xdc00],
        &[0x00, 0x31],
        &[0x22, 0x5c, 0x22],
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

#[test]
fn json_string_matches_node_json_stringify() {
    let cases = corpus();
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
    let expected: Vec<&str> = std::str::from_utf8(&out.stdout).unwrap().lines().collect();
    assert_eq!(expected.len(), cases.len());
    for (units, want) in cases.iter().zip(expected) {
        assert_eq!(json_string(units.iter().copied()), want, "{units:x?}");
    }
}
