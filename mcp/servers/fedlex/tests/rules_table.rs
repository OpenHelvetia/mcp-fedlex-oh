//! The gate over `docs/reference/fedlex-data-rules.md` (BV, sharpened
//! at BV part A′).
//!
//! The reference page states the two Fedlex rulebooks as rules and then
//! claims, per rule, whether this server honours them. A claim is cheap;
//! this test makes it expensive. It fails if
//!
//! 1. a row marked `honoured` names something that is not a TEST — a
//!    function this crate does not carry, a helper without `#[test]`, or
//!    a recorder carrying `#[ignore]` (the recording passes are named
//!    like tests and prove nothing offline);
//! 2. a row marked `honoured` names no test at all;
//! 3. a row marked `untested` names a function that does not exist —
//!    a stale name is a stale claim either way;
//! 4. the rules sections and the conformance table disagree about which
//!    rules exist, in either direction or in their order;
//! 5. a row carries a status outside the closed set.
//!
//! The checks themselves are tested (`the_gate_*`), over synthetic page
//! text and synthetic sources, so «the gate bites» is a test rather than
//! a sentence in a report.
//!
//! It reads files and parses lines: no dependency beyond the crate's
//! own, no network, deterministic.

use std::collections::BTreeSet;
use std::path::PathBuf;

const STATUSES: [&str; 5] = [
    "honoured",
    "violated",
    "untested",
    "not_applicable",
    "unknown",
];

fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn page() -> String {
    let path = repo_root().join("docs/reference/fedlex-data-rules.md");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is part of the gate and must exist: {e}", path.display()))
}

/// The ids the rules sections declare, in order of appearance.
fn rule_ids(page: &str) -> Vec<String> {
    page.lines()
        .filter_map(|line| line.strip_prefix("### "))
        .filter_map(|rest| rest.split_once(" — "))
        .map(|(id, _)| id.trim().to_string())
        .collect()
}

/// One parsed row of the conformance table.
struct Row {
    id: String,
    tests: Vec<String>,
    status: String,
}

fn table_rows(page: &str) -> Vec<Row> {
    page.lines()
        .filter(|line| line.starts_with("| `"))
        .filter_map(|line| {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            if cells.len() < 5 {
                return None;
            }
            let id = cells[0].trim_matches('`').to_string();
            // The two closing tables carry the same id shape but fewer
            // columns; the length check above is what separates them.
            if !(id.starts_with('J') || id.starts_with('X')) {
                return None;
            }
            let tests = cells[2]
                .split(';')
                .map(str::trim)
                .filter(|t| !t.is_empty() && *t != "—")
                .map(str::to_string)
                .collect();
            Some(Row {
                id,
                tests,
                status: cells[3].to_string(),
            })
        })
        .collect()
}

/// Is `function` defined in `source` — and is it a test?
///
/// A row may name only a function the suite actually RUNS: the
/// attributes immediately above the definition must carry `#[test]` or
/// `#[tokio::test]`, and `#[ignore]` disqualifies it (the recording
/// passes are `#[ignore]`d and answer nothing offline).
fn function_is_a_test(source: &str, function: &str) -> Result<(), String> {
    let lines: Vec<&str> = source.lines().collect();
    let needle = format!("fn {function}(");
    let Some(at) = lines
        .iter()
        .position(|line| line.trim_start().starts_with(&needle))
    else {
        return Err(format!("no «fn {function}»"));
    };
    let mut attributes = Vec::new();
    for line in lines[..at].iter().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with("#[") {
            attributes.push(trimmed);
            continue;
        }
        if trimmed.starts_with("///") || trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }
        break;
    }
    if attributes.iter().any(|a| a.starts_with("#[ignore")) {
        return Err(format!(
            "«fn {function}» carries #[ignore] — a recording pass proves nothing offline"
        ));
    }
    if attributes
        .iter()
        .any(|a| *a == "#[test]" || *a == "#[tokio::test]")
    {
        Ok(())
    } else {
        Err(format!(
            "«fn {function}» carries no #[test] — a helper is not a proof"
        ))
    }
}

/// The problems of one page, with the sources resolved by `source_of`
/// (the file half of a `<file>::<function>` reference). Split out so the
/// gate's own checks can be tested on synthetic input.
fn problems_of(page: &str, source_of: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let mut problems = Vec::new();
    let rows = table_rows(page);
    for row in &rows {
        if !STATUSES.contains(&row.status.as_str()) {
            problems.push(format!(
                "{}: status «{}» is outside the closed set {STATUSES:?}",
                row.id, row.status
            ));
        }
        if row.status == "honoured" && row.tests.is_empty() {
            problems.push(format!(
                "{}: claims «honoured» and names no test — that is «untested»",
                row.id
            ));
        }
        // An honoured row must name tests; an untested row may name a
        // test it does not rely on, but the name must still be real.
        if row.status != "honoured" && row.status != "untested" {
            continue;
        }
        for reference in &row.tests {
            let Some((file, function)) = reference.split_once("::") else {
                problems.push(format!(
                    "{}: «{reference}» is not «<file>::<function>»",
                    row.id
                ));
                continue;
            };
            let Some(source) = source_of(file) else {
                problems.push(format!("{}: «{reference}»: {file} does not exist", row.id));
                continue;
            };
            match (row.status.as_str(), function_is_a_test(&source, function)) {
                (_, Ok(())) => {}
                ("honoured", Err(why)) => problems.push(format!("{}: {file}: {why}", row.id)),
                // An untested row is only held to the name existing.
                ("untested", Err(why)) if why.starts_with("no «fn") => {
                    problems.push(format!("{}: {file}: {why}", row.id))
                }
                _ => {}
            }
        }
    }
    problems
}

fn ids_disagree(page: &str) -> Vec<String> {
    let stated = rule_ids(page);
    let tabled: Vec<String> = table_rows(page).into_iter().map(|r| r.id).collect();
    let stated_set: BTreeSet<&String> = stated.iter().collect();
    let tabled_set: BTreeSet<&String> = tabled.iter().collect();
    let mut problems = Vec::new();
    if stated.len() != stated_set.len() {
        problems.push(format!("a rule id is stated twice: {stated:?}"));
    }
    if tabled.len() != tabled_set.len() {
        problems.push(format!("a rule id has two rows: {tabled:?}"));
    }
    for missing in stated_set.difference(&tabled_set) {
        problems.push(format!("restated without a row: {missing}"));
    }
    for missing in tabled_set.difference(&stated_set) {
        problems.push(format!("a row for a rule that is not restated: {missing}"));
    }
    if problems.is_empty() && stated != tabled {
        problems.push("the table's order must follow the rules' order".to_string());
    }
    problems
}

fn source_from_disk(file: &str) -> Option<String> {
    std::fs::read_to_string(crate_root().join(file)).ok()
}

#[test]
fn the_conformance_table_claims_only_what_the_suite_runs() {
    let page = page();
    let rows = table_rows(&page);
    assert!(
        rows.len() > 100,
        "the table lost its rows: {} parsed",
        rows.len()
    );
    let problems = problems_of(&page, &source_from_disk);
    assert!(
        problems.is_empty(),
        "the conformance table claims what the suite does not run:\n  {}",
        problems.join("\n  ")
    );
}

#[test]
fn every_restated_rule_has_a_row_and_every_row_a_rule() {
    let problems = ids_disagree(&page());
    assert!(
        problems.is_empty(),
        "the rules and the table disagree:\n  {}",
        problems.join("\n  ")
    );
}

// --- the gate's own proofs, on synthetic pages and sources ------------

const SOURCE: &str = r#"
/// A real test.
#[test]
fn a_real_test() { assert!(true); }

/// A helper the suite calls but never runs on its own.
fn a_helper() -> bool { true }

/// A recording pass: named like a test, ignored by the suite.
#[test]
#[ignore = "hits the live endpoint"]
fn record_something() {}
"#;

fn synthetic_page(test_cell: &str, status: &str) -> String {
    format!(
        "### J1.1 — a rule\n\nstatement\n\n\
         ## 4. Conformance table\n\n\
         | id | tool(s) | test (file::function) | status | note |\n\
         |---|---|---|---|---|\n\
         | `J1.1` | a_tool | {test_cell} | {status} | note |\n"
    )
}

fn synthetic_source(_file: &str) -> Option<String> {
    Some(SOURCE.to_string())
}

#[test]
fn the_gate_refuses_a_test_that_does_not_exist() {
    let page = synthetic_page("tests/e2e.rs::a_test_nobody_wrote", "honoured");
    let problems = problems_of(&page, &synthetic_source);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("no «fn a_test_nobody_wrote»"),
        "{problems:?}"
    );
    // The same name in an untested row is refused too — a stale name is
    // a stale claim either way.
    let untested = synthetic_page("tests/e2e.rs::a_test_nobody_wrote", "untested");
    assert_eq!(problems_of(&untested, &synthetic_source).len(), 1);
    // And the real one passes.
    let good = synthetic_page("tests/e2e.rs::a_real_test", "honoured");
    assert!(problems_of(&good, &synthetic_source).is_empty());
}

#[test]
fn the_gate_refuses_an_honoured_row_without_a_test() {
    let page = synthetic_page("—", "honoured");
    let problems = problems_of(&page, &synthetic_source);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("names no test"), "{problems:?}");
    // An untested row without a test is the honest state, not a problem.
    assert!(problems_of(&synthetic_page("—", "untested"), &synthetic_source).is_empty());
}

#[test]
fn the_gate_refuses_a_helper_named_as_a_test() {
    let page = synthetic_page("tests/e2e.rs::a_helper", "honoured");
    let problems = problems_of(&page, &synthetic_source);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("carries no #[test]"), "{problems:?}");
}

#[test]
fn the_gate_refuses_an_ignored_recorder_named_as_a_test() {
    let page = synthetic_page("tests/e2e.rs::record_something", "honoured");
    let problems = problems_of(&page, &synthetic_source);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("#[ignore]"), "{problems:?}");
}

#[test]
fn the_gate_refuses_a_rule_and_a_row_that_disagree() {
    // A rule without a row …
    let page = "### J1.1 — a rule\n\n### J1.2 — another\n\n\
                | `J1.1` | t | — | untested | n |\n";
    let problems = ids_disagree(page);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("restated without a row: J1.2"),
        "{problems:?}"
    );
    // … and a row without a rule.
    let other = "### J1.1 — a rule\n\n\
                 | `J1.1` | t | — | untested | n |\n\
                 | `J9.9` | t | — | untested | n |\n";
    let problems = ids_disagree(other);
    assert!(
        problems[0].contains("a row for a rule that is not restated: J9.9"),
        "{problems:?}"
    );
    // An unknown status is refused as well.
    let bad_status = synthetic_page("—", "probably-fine");
    let problems = problems_of(&bad_status, &synthetic_source);
    assert!(
        problems[0].contains("outside the closed set"),
        "{problems:?}"
    );
}
