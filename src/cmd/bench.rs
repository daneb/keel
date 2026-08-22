//! `keel bench` — the measurement the Phase 4 exit criterion demands.
//!
//! > Measured token-per-task drop on a fixed set of five representative tasks,
//! > before/after, on the same model. … If you cannot measure it, do not ship
//! > it — the literature here is full of vendor numbers and at least one
//! > publicly retracted benchmark.
//!
//! So: no vendor numbers here. The comparison is between two concrete things
//! keel can both produce — the tokens a retrieval answer costs, against the
//! tokens the file reads that would otherwise answer the same question cost.
//! Both sides are counted with the same estimator, on this repository, now.
//!
//! What this does **not** measure is answer quality. The published work that
//! reports ~10× fewer tokens also reports 83% answer quality against 92%. A
//! token ratio is a cost measurement, and is reported as one.

use crate::config::Config;
use crate::paths::Paths;
use crate::retrieve::Retriever;
use crate::trajectory::event::estimate_tokens;
use anyhow::Result;
use serde::Serialize;

/// One question, and the files a reader would have had to open to answer it.
struct Task {
    question: &'static str,
    /// The retrieval calls that answer it.
    retrieval: Vec<Query>,
    /// The files an agent without an index would read to answer the same thing.
    baseline_files: Vec<&'static str>,
    /// The files a correct answer must surface. Retrieval that saves tokens by
    /// not mentioning these has not saved anything — it has just answered less.
    answer_files: Vec<&'static str>,
}

enum Query {
    Outline(&'static str),
    Symbol(&'static str),
    Refs(&'static str),
    Importers(&'static str),
}

#[derive(Debug, Serialize)]
pub struct TaskResult {
    pub question: String,
    pub retrieval_tokens: usize,
    pub baseline_tokens: usize,
    pub ratio: f64,
    pub baseline_files: usize,
    /// Fraction of the answer files retrieval actually surfaced.
    pub recall: f64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missed: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BenchResult {
    pub tasks: Vec<TaskResult>,
    pub retrieval_total: usize,
    pub baseline_total: usize,
    pub ratio: f64,
    pub recall: f64,
    pub indexed_files: usize,
    pub indexed_symbols: usize,
}

/// Five representative questions about this repository.
///
/// Fixed deliberately: a benchmark whose tasks move is a benchmark you can
/// tune until it says what you want.
fn tasks() -> Vec<Task> {
    vec![
        Task {
            question: "What is the public surface of the projection layer?",
            retrieval: vec![
                Query::Outline("src/projection/mod.rs"),
                Query::Outline("src/projection/drift.rs"),
            ],
            baseline_files: vec!["src/projection/mod.rs", "src/projection/drift.rs"],
            answer_files: vec!["src/projection/mod.rs", "src/projection/drift.rs"],
        },
        Task {
            question: "Where is `store_hash` defined and who calls it?",
            retrieval: vec![Query::Symbol("store_hash"), Query::Refs("store_hash")],
            baseline_files: vec![
                "src/store/mod.rs", "src/projection/mod.rs", "src/cmd/store.rs",
                "src/gate/g0.rs", "src/gate/g2.rs", "src/cmd/status.rs",
            ],
            answer_files: vec!["src/store/mod.rs", "src/gate/g0.rs", "src/gate/g2.rs"],
        },
        Task {
            question: "What breaks if the Paths type changes?",
            retrieval: vec![Query::Importers("src/paths.rs"), Query::Refs("Paths")],
            baseline_files: vec![
                "src/paths.rs", "src/store/mod.rs", "src/map/mod.rs",
                "src/projection/mod.rs", "src/cmd/init.rs", "src/cmd/status.rs",
                "src/gate/g0.rs", "src/gate/g1.rs",
            ],
            answer_files: vec!["src/paths.rs", "src/store/mod.rs", "src/map/mod.rs"],
        },
        Task {
            question: "How does a gate verdict get recorded?",
            retrieval: vec![
                Query::Symbol("GateResult"),
                Query::Refs("GateResult"),
                Query::Outline("src/gate/mod.rs"),
            ],
            baseline_files: vec!["src/gate/mod.rs", "src/cmd/gate.rs", "src/run.rs"],
            answer_files: vec!["src/gate/mod.rs", "src/cmd/gate.rs"],
        },
        Task {
            question: "What does the failure classifier do with a blocked check?",
            retrieval: vec![Query::Symbol("classify"), Query::Outline("src/failure/mod.rs")],
            baseline_files: vec!["src/failure/mod.rs", "src/failure/taxonomy.rs"],
            answer_files: vec!["src/failure/mod.rs"],
        },
    ]
}

pub fn run(json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let r = Retriever::open(&paths)?;
    if let Some(why) = &r.degraded {
        anyhow::bail!("{why} — the benchmark needs an index to measure against");
    }
    let (indexed_files, indexed_symbols) = r.totals()?;
    let _ = cfg;

    let mut results = Vec::new();
    for t in tasks() {
        // Retrieval side: unbudgeted, so the comparison is like for like. A
        // truncated answer would flatter retrieval by hiding the cost.
        let mut retrieval_tokens = 0;
        let mut surfaced = String::new();
        for q in &t.retrieval {
            let a = match q {
                Query::Outline(p) => r.outline(p)?,
                Query::Symbol(n) => r.symbol(n)?,
                Query::Refs(n) => r.refs(n)?,
                Query::Importers(p) => r.importers(p)?,
            };
            retrieval_tokens += a.tokens;
            surfaced.push_str(&a.text);
        }

        // Recall: did the cheaper answer actually name the files that hold the
        // answer? A token saving achieved by answering less is not a saving.
        let missed: Vec<String> = t
            .answer_files
            .iter()
            .filter(|f| !surfaced.contains(**f))
            .map(|f| f.to_string())
            .collect();
        let recall = if t.answer_files.is_empty() {
            1.0
        } else {
            1.0 - missed.len() as f64 / t.answer_files.len() as f64
        };

        let mut baseline_tokens = 0;
        for f in &t.baseline_files {
            let abs = paths.repo.join(f);
            match std::fs::read_to_string(&abs) {
                Ok(c) => baseline_tokens += estimate_tokens(&c),
                Err(_) => anyhow::bail!("benchmark file {f} is missing; the tasks are stale"),
            }
        }

        results.push(TaskResult {
            question: t.question.to_string(),
            retrieval_tokens,
            baseline_tokens,
            ratio: if retrieval_tokens == 0 { 0.0 } else { baseline_tokens as f64 / retrieval_tokens as f64 },
            baseline_files: t.baseline_files.len(),
            recall,
            missed,
        });
    }

    let retrieval_total: usize = results.iter().map(|r| r.retrieval_tokens).sum();
    let baseline_total: usize = results.iter().map(|r| r.baseline_tokens).sum();
    let recall = if results.is_empty() {
        0.0
    } else {
        results.iter().map(|r| r.recall).sum::<f64>() / results.len() as f64
    };
    let bench = BenchResult {
        recall,
        ratio: if retrieval_total == 0 { 0.0 } else { baseline_total as f64 / retrieval_total as f64 },
        tasks: results,
        retrieval_total,
        baseline_total,
        indexed_files,
        indexed_symbols,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&bench)?);
        return Ok(0);
    }

    println!(
        "keel bench — {} files, {} symbols indexed\n",
        bench.indexed_files, bench.indexed_symbols
    );
    println!("{:<48} {:>10} {:>10} {:>7} {:>7}", "task", "retrieval", "read", "ratio", "recall");
    println!("{}", "-".repeat(86));
    for t in &bench.tasks {
        println!(
            "{:<48} {:>10} {:>10} {:>6.1}× {:>6.0}%",
            truncate(&t.question, 46),
            t.retrieval_tokens,
            t.baseline_tokens,
            t.ratio,
            t.recall * 100.0
        );
        for m in &t.missed {
            println!("{:<48} missed {m}", "");
        }
    }
    println!("{}", "-".repeat(86));
    println!(
        "{:<48} {:>10} {:>10} {:>6.1}× {:>6.0}%",
        "total", bench.retrieval_total, bench.baseline_total, bench.ratio, bench.recall * 100.0
    );
    println!(
        "\nTokens estimated at 4 chars each, both sides, on this repository.\n\
         \n\
         Recall is narrow: it asks whether the cheap answer named the files that\n\
         hold the answer, because a token saving achieved by answering less is\n\
         not a saving. It does not measure whether a model would then answer\n\
         correctly — that needs a task set with known-good answers, which this\n\
         is not. Phase 4 accepts a ratio at or above 3×."
    );
    if bench.ratio < 3.0 {
        println!("\n  BELOW TARGET — {:.1}× is under the 3× the plan accepts.", bench.ratio);
        return Ok(1);
    }
    if bench.recall < 1.0 {
        // Not a failure — but a saving bought by missing the answer is worth
        // seeing next to the ratio it inflated.
        println!(
            "\n  {:.0}% recall — retrieval missed a file a correct answer needs.",
            bench.recall * 100.0
        );
    }
    Ok(0)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max - 1).chain(['…']).collect()
}
