//! The comparison page keeps its promises: fixed shape, dated sources, linked evidence.

use std::fs;
use std::path::Path;

const TOOLS: [&str; 6] = [
    "spec-kit",
    "Kiro",
    "BMAD",
    "Tessl",
    "Elephant-Goldfish",
    "Claude Code hooks",
];
const ROWS: [&str; 8] = [
    "falsifiable criteria",
    "blocked as a verdict",
    "approvals bound to content",
    "tamper-evident evidence",
    "computed blast radius",
    "test-tampering review",
    "lessons enforced as checks",
    "agent drivers",
];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn doc() -> String {
    fs::read_to_string(root().join("docs/comparison.md")).expect("docs/comparison.md")
}

/// Table rows after the header and separator, split into trimmed cells.
fn table(doc: &str) -> Vec<Vec<String>> {
    doc.lines()
        .filter(|l| l.starts_with('|'))
        .map(|l| {
            l.trim_matches('|')
                .split(" | ")
                .map(|c| c.trim().trim_matches('|').trim().to_string())
                .collect()
        })
        .collect()
}

fn section<'a>(doc: &'a str, heading: &str) -> &'a str {
    let start = doc.find(heading).expect("section present");
    let rest = &doc[start + heading.len()..];
    rest.split("\n## ").next().unwrap_or(rest)
}

#[test]
fn the_table_has_the_fixed_columns_and_rows() {
    let t = table(&doc());
    let mut header = vec!["", "keel"];
    header.extend(TOOLS);
    assert_eq!(t[0], header);
    let labels: Vec<&str> = t[2..].iter().map(|r| r[0].as_str()).collect();
    assert_eq!(labels, ROWS);
    assert!(t[2..].iter().all(|r| r.len() == 8), "every row has 8 cells");
}

#[test]
fn every_other_tool_cell_footnotes_a_dated_https_source() {
    let d = doc();
    let t = table(&d);
    let sources = section(&d, "## Sources");
    for row in &t[2..] {
        for cell in &row[2..] {
            let start = cell.rfind("[^").expect("cell has a footnote");
            let id = &cell[start..cell[start..].find(']').unwrap() + start + 1];
            let def = sources
                .lines()
                .find(|l| l.starts_with(&format!("{id}:")))
                .unwrap_or_else(|| panic!("no source for {id}"));
            assert!(def.contains("https://"), "{def}");
            let date = def.split("checked: ").nth(1).expect("checked date");
            let b = date.as_bytes();
            assert!(
                b.len() >= 10 && b[4] == b'-' && b[7] == b'-'
                    && date[..10].chars().filter(char::is_ascii_digit).count() == 8,
                "{def}"
            );
        }
    }
}

#[test]
fn every_keel_claim_links_to_an_existing_adr_or_test() {
    let d = doc();
    for row in &table(&d)[2..] {
        let cell = &row[1];
        let target = cell
            .split("](")
            .nth(1)
            .and_then(|s| s.split(')').next())
            .unwrap_or_else(|| panic!("keel cell without a link: {cell}"));
        let rel = target.trim_start_matches("../");
        assert!(
            rel.starts_with(".keel/store/decisions/") || rel.starts_with("tests/"),
            "{target}"
        );
        assert!(root().join(rel).exists(), "{target} does not exist");
    }
}

#[test]
fn the_weaker_section_names_every_compared_tool() {
    let d = doc();
    let s = section(&d, "## Where keel is weaker");
    for tool in TOOLS {
        let entry = s
            .lines()
            .find(|l| l.starts_with(&format!("- **{tool}**")))
            .unwrap_or_else(|| panic!("no entry for {tool}"));
        assert!(entry.contains("choose"), "{tool} entry names no situation");
    }
}

#[test]
fn readme_links_the_comparison_before_the_first_diagram() {
    let r = fs::read_to_string(root().join("README.md")).unwrap();
    let link = r.find("docs/comparison.md").expect("README links comparison");
    let diagram = r.find("<img").expect("README has a diagram");
    assert!(link < diagram);
}
