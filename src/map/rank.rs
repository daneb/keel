//! Importance ranking over the file graph (the Aider repo-map lineage: build a
//! graph, run PageRank, then fit the result to a token budget).
//!
//! Edge direction is importer → imported, so a file that many others depend on
//! accumulates rank. Two adjustments on top of vanilla PageRank:
//!
//! * **entry-point prior** — `main.rs`, `index.ts`, `__init__.py` and friends
//!   are where a reader starts even when nothing imports them, and
//! * **substance prior** — a file with symbols outranks an equally-connected
//!   file with none, so barrel/re-export files do not crowd out real code.

const DAMPING: f64 = 0.85;
const ITERATIONS: usize = 30;
const EPSILON: f64 = 1e-9;

pub struct Graph {
    pub n: usize,
    /// out_edges[i] = files that i imports
    pub out_edges: Vec<Vec<usize>>,
}

impl Graph {
    pub fn new(n: usize) -> Self {
        Self { n, out_edges: vec![Vec::new(); n] }
    }

    pub fn add_edge(&mut self, from: usize, to: usize) {
        if from == to || from >= self.n || to >= self.n { return; }
        if !self.out_edges[from].contains(&to) {
            self.out_edges[from].push(to);
        }
    }

    pub fn in_degree(&self) -> Vec<usize> {
        let mut d = vec![0usize; self.n];
        for outs in &self.out_edges {
            for &t in outs {
                d[t] += 1;
            }
        }
        d
    }

    pub fn pagerank(&self, personalisation: &[f64]) -> Vec<f64> {
        if self.n == 0 { return vec![]; }
        let total: f64 = personalisation.iter().sum();
        let prior: Vec<f64> = if total > 0.0 {
            personalisation.iter().map(|p| p / total).collect()
        } else {
            vec![1.0 / self.n as f64; self.n]
        };

        let mut rank = prior.clone();
        for _ in 0..ITERATIONS {
            let mut next = vec![0.0f64; self.n];
            let mut dangling = 0.0f64;
            for (i, outs) in self.out_edges.iter().enumerate() {
                if outs.is_empty() {
                    dangling += rank[i];
                    continue;
                }
                let share = rank[i] / outs.len() as f64;
                for &t in outs {
                    next[t] += share;
                }
            }
            let mut delta = 0.0;
            for i in 0..self.n {
                let v = DAMPING * (next[i] + dangling * prior[i]) + (1.0 - DAMPING) * prior[i];
                delta += (v - rank[i]).abs();
                next[i] = v;
            }
            rank = next;
            if delta < EPSILON { break; }
        }
        rank
    }
}

/// Prior weight for a file, before any graph structure is considered.
pub fn prior(rel: &str, symbol_count: usize, lines: usize) -> f64 {
    let base = 1.0;
    let entry = if is_entry_point(rel) { 2.5 } else { 0.0 };
    // Diminishing returns: a 4000-line file is not 40x more important than a
    // 100-line one, but it is more important.
    let substance = (symbol_count as f64).sqrt() * 0.6 + (lines as f64).sqrt() * 0.05;
    let depth_penalty = 1.0 / (1.0 + rel.matches('/').count() as f64 * 0.15);
    let test_penalty = if is_test(rel) { 0.25 } else { 1.0 };
    (base + entry + substance) * depth_penalty * test_penalty
}

pub fn is_entry_point(rel: &str) -> bool {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    matches!(
        base,
        "main.rs" | "lib.rs" | "mod.rs" | "main.go" | "main.py" | "__init__.py"
            | "index.ts" | "index.js" | "index.tsx" | "app.ts" | "app.py"
            | "Main.java" | "Application.java" | "cli.rs" | "server.ts"
    )
}

pub fn is_test(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    lower.starts_with("test/") || lower.starts_with("tests/")
        || lower.contains("/test/") || lower.contains("/tests/")
        || lower.contains("__tests__")
        || lower.ends_with("_test.go") || lower.ends_with("_test.py")
        || lower.ends_with(".test.ts") || lower.ends_with(".test.js")
        || lower.ends_with(".spec.ts") || lower.ends_with(".spec.js")
        || lower.ends_with("test.java")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depended_upon_files_outrank_leaves() {
        // 0 and 1 both import 2; nothing imports 0 or 1.
        let mut g = Graph::new(3);
        g.add_edge(0, 2);
        g.add_edge(1, 2);
        let r = g.pagerank(&[1.0, 1.0, 1.0]);
        assert!(r[2] > r[0] && r[2] > r[1], "hub should outrank leaves: {r:?}");
    }

    #[test]
    fn ranks_sum_to_one() {
        let mut g = Graph::new(4);
        g.add_edge(0, 1);
        g.add_edge(1, 2);
        let r = g.pagerank(&[1.0; 4]);
        let sum: f64 = r.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6, "sum was {sum}");
    }

    #[test]
    fn personalisation_breaks_ties() {
        let g = Graph::new(2); // no edges at all
        let r = g.pagerank(&[3.0, 1.0]);
        assert!(r[0] > r[1]);
    }

    #[test]
    fn entry_points_and_tests_are_recognised() {
        assert!(is_entry_point("src/main.rs"));
        assert!(!is_entry_point("src/api/routes.rs"));
        assert!(is_test("tests/api_test.rs"));
        assert!(is_test("web/__tests__/a.ts"));
        assert!(!is_test("src/api/routes.rs"));
    }

    #[test]
    fn prior_prefers_substance_and_shallow_paths() {
        assert!(prior("src/main.rs", 5, 200) > prior("src/a/b/c/util.rs", 5, 200));
        assert!(prior("src/api.rs", 20, 400) > prior("src/api.rs", 1, 400));
        assert!(prior("src/api.rs", 20, 400) > prior("tests/api.rs", 20, 400));
    }
}
