//! SD-1 part A: case_key format, the decisions registry (`lwc-rust-migrate/decisions.tsv`), and the ONE decisions
//! source, the case_key → D-id map `tools/semdiff/decisions.tsv` [AB-D0003 (3)].
//!
//! Id rules are the coverage gate's spec (`notes/arch-board/coverage-gate-spec.md` §Checks, kinds DEC-ID-INVALID
//! and DEC-UNRESOLVED), implemented here and only here:
//!   - registry: the `id` column is found by header name; every row's id must match `^D-[0-9]{4}$` and be unique,
//!     whether or not anything cites it (DEC-ID-INVALID); a registry without an `id` column resolves nothing
//!     (DEC-UNRESOLVED);
//!   - a cited id resolves only by exact match on that column (DEC-UNRESOLVED), never by substring.
//!
//! semdiff adds, per AB-D0003 (3): the cited row's `result` must be exactly `locked` (DEC-NOT-LOCKED).
//! In `semdiff run` every violation is an internal error (exit 2, no summary), never a silent pass.
//!
//! AB-L0030: [`Registry::check`] is the ONE registry validator. `semdiff check-registry` exposes it for
//! coverage-gate.mjs, which must not re-parse decisions.tsv (coverage-gate-spec "One validator").

use std::collections::BTreeMap;

use crate::sha256_hex;

/// The map, compiled into the binary so there is exactly one decisions source and it changes only by landing.
pub const DECISIONS_MAP_PATH: &str = "tools/semdiff/decisions.tsv";
pub const DECISIONS_MAP_TSV: &str = include_str!("../decisions.tsv");
pub const DECISIONS_MAP_HEADER: &str = "case_key\tdecision_id\twhy";

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A corpus name: the first case_key segment and the manifest `corpus`.
pub fn check_corpus_name(c: &str) -> Result<(), String> {
    if !c.is_empty() && c.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) && c != "." && c != ".." {
        Ok(())
    } else {
        Err(format!("corpus {c:?} must be non-empty [A-Za-z0-9._-]+ (it is the first case_key segment)"))
    }
}

/// case_key = `corpus/source/sha256(template)/sha256(opts)` (design §2): lowercase 64-hex hashes, `source` one or more
/// non-empty `/`-separated segments (no `.`/`..`), and no whitespace or control characters anywhere, so a key is
/// always one TSV field. Returns the corpus segment. semdiff cannot recompute the hashes (the manifest carries
/// compiled outputs, not templates or options), so the adapter (H-1) owns their correctness; semdiff owns the shape.
pub fn check_case_key(key: &str) -> Result<&str, String> {
    let bad = |why: &str| Err(format!("case_key {key:?}: {why} (want corpus/source/sha256(template)/sha256(opts))"));
    if key.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return bad("contains whitespace or a control character");
    }
    let mut back = key.rsplitn(3, '/');
    let (Some(opts), Some(tmpl), Some(rest)) = (back.next(), back.next(), back.next()) else { return bad("too few segments") };
    if !is_hex64(tmpl) || !is_hex64(opts) {
        return bad("the last two segments must be lowercase 64-hex sha256");
    }
    let Some((corpus, source)) = rest.split_once('/') else { return bad("missing source segment") };
    if check_corpus_name(corpus).is_err() {
        return bad("bad corpus segment");
    }
    if source.split('/').any(|s| s.is_empty() || s == "." || s == "..") {
        return bad("source has an empty, `.` or `..` segment");
    }
    Ok(corpus)
}

/// `^D-[0-9]{4}$` (coverage-gate-spec DEC-ID-INVALID).
pub fn is_decision_id(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 6 && b.starts_with(b"D-") && b[2..].iter().all(u8::is_ascii_digit)
}

/// One finding of the registry validator. `code` is one of the [`FindingCode`] spellings; `line` is 1-based.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub code: FindingCode,
    pub line: Option<usize>,
    pub id: Option<String>,
    pub detail: String,
}

/// Finding codes. The first two make the registry itself invalid (`check-registry` exit 2); the last two are about a
/// required id (exit 1). Spellings are part of the `semdiff-registry-check/v1` contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingCode {
    /// The file is unusable: unreadable, not UTF-8, empty (no header), or a header naming a column twice.
    Malformed,
    /// A row's id is missing, does not match `^D-[0-9]{4}$`, or repeats (coverage-gate-spec DEC-ID-INVALID).
    DecIdInvalid,
    /// A required id is not exactly the id of any row (coverage-gate-spec DEC-UNRESOLVED).
    DecUnresolved,
    /// A required id resolves but that row's `result` is not exactly `locked` (AB-D0003 (3)).
    DecNotLocked,
}

impl FindingCode {
    pub fn as_str(self) -> &'static str {
        match self {
            FindingCode::Malformed => "MALFORMED",
            FindingCode::DecIdInvalid => "DEC-ID-INVALID",
            FindingCode::DecUnresolved => "DEC-UNRESOLVED",
            FindingCode::DecNotLocked => "DEC-NOT-LOCKED",
        }
    }

    /// True for the codes that make the registry itself invalid.
    pub fn is_defect(self) -> bool {
        matches!(self, FindingCode::Malformed | FindingCode::DecIdInvalid)
    }
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code.as_str())?;
        if let Some(n) = self.line {
            write!(f, " (line {n})")?;
        }
        write!(f, ": {}", self.detail)
    }
}

/// `lwc-rust-migrate/decisions.tsv`. [`Registry::check`] is THE registry validator (AB-L0030): `semdiff run` and
/// `semdiff check-registry` (which coverage-gate.mjs calls) both go through it.
pub struct Registry {
    pub sha256: String,
    /// False when the header has no `id` column: then nothing resolves (coverage-gate-spec: DEC-UNRESOLVED on every
    /// cite), which is a resolution failure, not a malformed file.
    pub has_id_column: bool,
    /// id → whether `result` is exactly `locked`.
    rows: BTreeMap<String, bool>,
}

impl Registry {
    /// Validate the whole registry, whether or not anything cites it. Returns every defect (MALFORMED,
    /// DEC-ID-INVALID), in line order; the registry is usable only when the list is empty.
    /// - `id` and `result` columns are found by header name. No `id` column: zero ids (every require is
    ///   DEC-UNRESOLVED). No `result` column, or a short row without a `result` field: those rows are not locked
    ///   (fail closed; every require of them is DEC-NOT-LOCKED).
    pub fn check(text: &str) -> (Registry, Vec<Finding>) {
        let mut findings = Vec::new();
        let mut rows = BTreeMap::new();
        let mut lines = text.lines();
        let header: Vec<&str> = lines.next().unwrap_or("").split('\t').collect();
        let id_col = header.iter().position(|h| *h == "id");
        if text.is_empty() || header == [""] {
            findings.push(Finding { code: FindingCode::Malformed, line: Some(1), id: None, detail: "empty registry: no header line".into() });
        } else if let Some(dup) = header.iter().enumerate().find(|(i, h)| header[..*i].contains(h)).map(|(_, h)| h) {
            findings.push(Finding { code: FindingCode::Malformed, line: Some(1), id: None, detail: format!("header names column {dup:?} twice") });
        } else if let Some(id_col) = id_col {
            let res_col = header.iter().position(|h| *h == "result");
            for (i, line) in lines.enumerate() {
                let f: Vec<&str> = line.split('\t').collect();
                let n = i + 2;
                let id = f.get(id_col).copied().unwrap_or("");
                if !is_decision_id(id) {
                    let detail = format!("id {id:?} does not match ^D-[0-9]{{4}}$");
                    findings.push(Finding { code: FindingCode::DecIdInvalid, line: Some(n), id: Some(id.into()), detail });
                    continue;
                }
                let locked = res_col.and_then(|c| f.get(c)) == Some(&"locked");
                if rows.insert(id.to_string(), locked).is_some() {
                    let detail = format!("duplicate id {id}");
                    findings.push(Finding { code: FindingCode::DecIdInvalid, line: Some(n), id: Some(id.into()), detail });
                }
            }
        }
        (Registry { sha256: sha256_hex(text.as_bytes()), has_id_column: id_col.is_some(), rows }, findings)
    }

    /// [`Registry::check`], failing on any defect (all of them, joined) — the form `semdiff run` uses.
    pub fn parse(text: &str) -> Result<Registry, String> {
        let (reg, findings) = Registry::check(text);
        if findings.is_empty() {
            Ok(reg)
        } else {
            Err(format!("registry: {}", findings.iter().map(Finding::to_string).collect::<Vec<_>>().join("; ")))
        }
    }

    /// None iff `id` resolves (exact match on the id column) to a row whose result is exactly `locked`.
    pub fn resolve(&self, id: &str) -> Option<Finding> {
        let f = |code, detail: String| Some(Finding { code, line: None, id: Some(id.into()), detail });
        match self.rows.get(id) {
            None if !self.has_id_column => f(FindingCode::DecUnresolved, format!("{id}: the registry has no `id` column")),
            None => f(FindingCode::DecUnresolved, format!("{id} is not an id in the registry")),
            Some(false) => f(FindingCode::DecNotLocked, format!("{id} resolves but its result is not `locked`")),
            Some(true) => None,
        }
    }

    pub fn require_locked(&self, id: &str) -> Result<(), String> {
        self.resolve(id).map_or(Ok(()), |f| Err(f.to_string()))
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn locked(&self) -> usize {
        self.rows.values().filter(|l| **l).count()
    }
}

/// The case_key → D-id map.
pub struct DecisionMap {
    pub sha256: String,
    map: BTreeMap<String, String>,
}

impl DecisionMap {
    pub fn parse(text: &str, reg: &Registry) -> Result<DecisionMap, String> {
        let mut lines = text.lines();
        if lines.next() != Some(DECISIONS_MAP_HEADER) {
            return Err(format!("decisions map: BAD-HEADER: first line must be exactly {DECISIONS_MAP_HEADER:?}"));
        }
        let mut map = BTreeMap::new();
        for (i, line) in lines.enumerate() {
            let at = |why: String| Err(format!("decisions map line {}: {why}", i + 2));
            let f: Vec<&str> = line.split('\t').collect();
            let [key, id, why] = f[..] else { return at(format!("BAD-LINE: {} fields, want 3", f.len())) };
            if let Err(e) = check_case_key(key) {
                return at(format!("BAD-KEY: {e}"));
            }
            if !is_decision_id(id) {
                return at(format!("DEC-ID-INVALID: {id:?} does not match ^D-[0-9]{{4}}$"));
            }
            if why.trim().is_empty() {
                return at("NO-EVIDENCE: empty `why`".into());
            }
            if let Err(e) = reg.require_locked(id) {
                return at(e);
            }
            if map.insert(key.to_string(), id.to_string()).is_some() {
                return at(format!("DUP-KEY: case_key {key} appears on more than one row"));
            }
        }
        Ok(DecisionMap { sha256: sha256_hex(text.as_bytes()), map })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}
