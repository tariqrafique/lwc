use super::*;

const SRC: &str = "0123456789";

fn run(edits: Vec<Edit>) -> Result<String, OverlapError> {
    apply(SRC, edits)
}

// ---- existing behavior unchanged ----
#[test]
fn disjoint_replaces_and_inserts() {
    let e = vec![
        Edit::replace(6, 8, "x"),
        Edit::insert(0, "<"),
        Edit::replace(1, 3, "ab"),
        Edit::insert(10, ">"),
    ];
    assert_eq!(run(e).unwrap(), "<0ab345x89>");
}

#[test]
fn same_offset_inserts_keep_emission_order() {
    let e = vec![
        Edit::insert(4, "a"),
        Edit::insert(4, "b"),
        Edit::insert(4, "c"),
    ];
    assert_eq!(run(e).unwrap(), "0123abc456789");
}

#[test]
fn boundary_inserts_around_a_replace_are_kept() {
    // insert at start goes before the replacement, insert at end after it (sorted by (start, end))
    let e = vec![
        Edit::insert(7, "]"),
        Edit::replace(2, 7, "R"),
        Edit::insert(2, "["),
        Edit::insert(7, ")"),
    ];
    assert_eq!(run(e).unwrap(), "01[R])789");
}

#[test]
fn adjacent_replaces_and_empty_list() {
    assert_eq!(
        run(vec![Edit::replace(0, 5, "A"), Edit::replace(5, 10, "B")]).unwrap(),
        "AB"
    );
    assert_eq!(run(vec![]).unwrap(), SRC);
    assert_eq!(run(vec![Edit::replace(3, 6, "")]).unwrap(), "0126789");
}

#[test]
fn piece_map_unchanged() {
    let (out, p) =
        apply_mapped(SRC, vec![Edit::insert(0, "<"), Edit::replace(2, 4, "xy")]).unwrap();
    assert_eq!(out, "<01xy456789");
    let got: Vec<_> = p
        .iter()
        .map(|p| (p.out_start, p.out_end, p.in_start, p.kept))
        .collect();
    assert_eq!(
        got,
        vec![
            (0, 1, 0, false),
            (1, 3, 0, true),
            (3, 5, 2, false),
            (5, 11, 4, true)
        ]
    );
}

// ---- contained edits are dropped ----
#[test]
fn insert_strictly_inside_removal_is_dropped() {
    let e = vec![Edit::insert(4, "!"), Edit::replace(2, 7, "R")];
    assert_eq!(run(e).unwrap(), "01R789");
}

#[test]
fn replace_inside_removal_is_dropped_in_either_emission_order() {
    assert_eq!(
        run(vec![Edit::replace(3, 5, "x"), Edit::replace(2, 7, "R")]).unwrap(),
        "01R789"
    );
    assert_eq!(
        run(vec![Edit::replace(2, 7, "R"), Edit::replace(3, 5, "x")]).unwrap(),
        "01R789"
    );
}

#[test]
fn contained_edit_sharing_one_boundary_is_dropped() {
    assert_eq!(
        run(vec![Edit::replace(2, 4, "x"), Edit::replace(2, 7, "R")]).unwrap(),
        "01R789"
    );
    assert_eq!(
        run(vec![Edit::replace(5, 7, "x"), Edit::replace(2, 7, "R")]).unwrap(),
        "01R789"
    );
}

#[test]
fn nested_removals_drop_to_the_outermost() {
    let e = vec![
        Edit::replace(4, 5, "i"),
        Edit::replace(3, 6, "m"),
        Edit::replace(1, 9, "O"),
        Edit::insert(4, "!"),
    ];
    assert_eq!(run(e).unwrap(), "0O9");
}

#[test]
fn edits_outside_a_removal_survive_with_it() {
    let e = vec![
        Edit::replace(0, 1, "a"),
        Edit::replace(3, 5, "x"),
        Edit::replace(2, 7, "R"),
        Edit::insert(8, "+"),
    ];
    assert_eq!(run(e).unwrap(), "a1R7+89");
}

// ---- partial overlap is an error ----
#[test]
fn partial_overlap_errors() {
    assert_eq!(
        run(vec![Edit::replace(2, 5, "a"), Edit::replace(4, 8, "b")]),
        Err(OverlapError {
            first: (2, 5),
            second: (4, 8)
        })
    );
    assert_eq!(
        run(vec![Edit::replace(4, 8, "b"), Edit::replace(2, 5, "a")]),
        Err(OverlapError {
            first: (2, 5),
            second: (4, 8)
        })
    );
}

#[test]
fn partial_overlap_inside_a_container_is_still_contained() {
    // they overlap each other, but both lie fully under the outer removal, so both are dropped
    let e = vec![
        Edit::replace(2, 5, "a"),
        Edit::replace(4, 8, "b"),
        Edit::replace(1, 9, "O"),
    ];
    assert_eq!(run(e).unwrap(), "0O9");
}

#[test]
fn equal_spans_error() {
    assert_eq!(
        run(vec![Edit::replace(2, 5, "a"), Edit::replace(2, 5, "b")]),
        Err(OverlapError {
            first: (2, 5),
            second: (2, 5)
        })
    );
}
