//! keel — a gated harness for daily AI-assisted delivery.
//!
//! Phase 0: one store, many agents. Everything here exists to make every AI
//! session in every tool start from the same, current, budget-bounded picture
//! of the repository — and to make drift in that picture loud.

mod approval;
mod cmd;
mod config;
mod driver;
mod evidence;
mod failure;
mod gate;
mod lesson;
mod hashing;
mod map;
mod mcp;
mod paths;
mod plan;
mod projection;
mod retrieve;
mod review;
mod run;
mod spec;
mod store;
mod worktree;
mod trajectory;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "keel",
    version,
    about = "A gated harness for AI-assisted delivery — one store, many agents",
    long_about = "keel is a conductor, not an agent loop. It owns the knowledge store,\n\
                  the structural map of the repository, and the projections that every\n\
                  coding agent reads — and it tells you when they have drifted apart."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold .keel/, seed the store, build the first map
    Init {
        /// Re-scaffold over an existing .keel/ (store files are kept)
        #[arg(long)]
        force: bool,
        /// Skip the interview and accept seeded defaults
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Rebuild the symbol index and the generated maps
    Map {
        /// Override map.budget_lines for this run
        #[arg(long)]
        budget: Option<usize>,
        /// Re-parse everything instead of reusing unchanged files
        #[arg(long)]
        full: bool,
        #[arg(long)]
        json: bool,
    },
    /// The knowledge store and its projections
    #[command(subcommand)]
    Store(StoreCmd),
    /// Show whether the store, map and projections are current
    Status,
    /// What should I do next? Inspects the pipeline and prints one step.
    Next {
        /// Spec slug; optional when there is only one active spec
        slug: Option<String>,
    },
    /// The harness measured across runs: pass rates, failures, tokens, theatre
    Metrics {
        /// Runs a check must survive without failing to count as theatre
        #[arg(long, default_value = "20")]
        threshold: usize,
        #[arg(long)]
        json: bool,
    },
    /// Check the health of the whole harness in one place
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Manage the pre-commit hook that enforces `store check`
    #[command(subcommand)]
    Hook(HookCmd),
    /// Author and inspect specs
    #[command(subcommand)]
    Spec(SpecCmd),
    /// Show a plan's tasks grouped into dependency waves
    Tasks {
        slug: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Compute the blast radius and scaffold plan.md + tasks.md
    Plan {
        /// Spec slug; optional when there is only one spec
        slug: Option<String>,
        /// How far to walk the reverse-import graph
        #[arg(long)]
        depth: Option<usize>,
    },
    /// Run a gate and record its verdict
    #[command(subcommand)]
    Gate(GateCmd),
    /// Record a human decision on a stage
    Approve {
        slug: Option<String>,
        /// Which stage is being signed off
        #[arg(long, default_value = "spec")]
        stage: String,
        /// Record a rejection instead of an approval
        #[arg(long)]
        reject: bool,
        /// Why
        #[arg(long)]
        note: Option<String>,
    },
    /// Show the approval history and current standing
    Approvals { slug: Option<String> },
    /// Execute a task through an agent, capture evidence, run G2/G2.5/G3
    Run {
        /// Spec slug; optional when there is only one spec
        slug: Option<String>,
        /// Task id from tasks.md, e.g. T-1
        #[arg(long)]
        task: Option<String>,
        /// Driver id; defaults to the configured default
        #[arg(long)]
        driver: Option<String>,
        /// Gate the working tree as it stands instead of invoking an agent
        #[arg(long)]
        no_driver: bool,
        /// Diff against this ref instead of the inferred base
        #[arg(long)]
        base: Option<String>,
        /// Run every task, wave by wave, each in its own git worktree
        #[arg(long)]
        waves: bool,
        #[arg(long)]
        json: bool,
    },
    /// Print a run's trajectory in sequence order
    Replay {
        run: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List runs
    Runs {
        /// Print only the most recent run id
        #[arg(long)]
        latest: bool,
        /// Remove old runs, keeping the most recent N and anything a lesson cites
        #[arg(long)]
        prune: bool,
        /// How many recent runs to keep when pruning
        #[arg(long, default_value = "20")]
        keep: usize,
        /// Actually delete; without this, prune only reports
        #[arg(long)]
        apply: bool,
    },
    /// Write an evidence bundle, or verify one
    Export {
        /// Run id; defaults to the most recent
        run: Option<String>,
        /// Verify this bundle instead of writing one
        #[arg(long)]
        verify: Option<String>,
        /// Directory to write the bundle into
        #[arg(long)]
        out: Option<String>,
    },
    /// Extract failure episodes, classify them, propose lesson cards
    Learn {
        /// Run id; defaults to the most recent
        run: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Failure episodes across every run, with the attribution distribution
    Failures {
        #[arg(long)]
        json: bool,
    },
    /// Lessons in force
    Lessons {
        #[arg(long)]
        json: bool,
    },
    /// Accept, decline or retire a lesson
    #[command(subcommand)]
    Lesson(LessonCmd),
    /// Record the current metrics as the baseline the ratchet holds
    Ratchet {
        /// Accept the current measurements as the new baseline
        #[arg(long)]
        accept: bool,
    },
    /// A file's skeleton — signatures, no bodies
    Outline {
        path: String,
        #[arg(long)]
        json: bool,
    },
    /// A symbol's signature, doc and location
    Symbol {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// A symbol's body, on demand
    Source {
        name: String,
        /// Which definition, when a name is defined more than once
        #[arg(long, default_value = "1")]
        nth: usize,
        /// Why the whole body is needed; required above the line limit
        #[arg(long)]
        justify: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Where a symbol is used
    Refs {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// Which files import a path
    Importers {
        path: String,
        #[arg(long)]
        json: bool,
    },
    /// Everything one task needs, budget-fitted
    Slice {
        /// Task id, e.g. T-1
        task: String,
        #[arg(long)]
        slug: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Inspect and conformance-check agent drivers
    #[command(subcommand)]
    Driver(DriverCmd),
    /// Serve the retrieval layer over MCP on stdio
    Mcp,
    /// Measure retrieval against reading whole files
    Bench {
        #[arg(long)]
        json: bool,
    },
    /// What else does this change touch?
    Blast {
        /// Path globs, e.g. `src/api/**`
        targets: Vec<String>,
        /// Start from the file(s) defining this symbol instead
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        depth: Option<usize>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum SpecCmd {
    /// Scaffold a new spec, then run G0 on it
    New {
        slug: String,
        /// Human-readable title; defaults to the slug
        #[arg(long)]
        title: Option<String>,
        /// Scope globs this change may touch (repeatable)
        #[arg(long)]
        scope: Vec<String>,
        #[arg(long)]
        force: bool,
    },
    /// List specs with their gate verdicts
    List,
    /// Print the authoring prompt for an agent
    Prompt { slug: String },
}

#[derive(Subcommand)]
enum DriverCmd {
    /// Configured drivers and whether keel can reach them
    List,
    /// Run the conformance suite against a driver, in a scratch repository
    Check {
        id: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Write the reference driver scripts into .keel/drivers/ and register
    /// any that are missing from keel.toml
    ///
    /// `keel init` already does this for a new repository. Run it directly
    /// when .keel/ exists but .keel/drivers/ is empty or missing an agent —
    /// keel init bails on an already-initialised repo rather than reaching
    /// into it, so this is the way back in.
    Scaffold {
        /// Overwrite scripts that already exist
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum LessonCmd {
    /// Promote candidate <n> from `keel learn` into a lesson card
    Promote {
        index: usize,
        /// Promote despite the promotion rules, deliberately
        #[arg(long)]
        force: bool,
    },
    /// Decline candidate <n>
    Reject {
        index: usize,
        #[arg(long)]
        note: Option<String>,
    },
    /// Retire a lesson that has decayed
    Demote {
        id: String,
        #[arg(long)]
        reason: Option<String>,
    },
}

#[derive(Subcommand)]
enum GateCmd {
    /// G0 — is this spec buildable?
    G0 {
        slug: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// G1 — is this plan bounded?
    G1 {
        slug: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// G2/G2.5/G3 — is the working tree verified, reviewed and decidable?
    ///
    /// These three judge a change rather than a document, so they run together
    /// over the tree as it stands. `keel run` runs the same gates after an
    /// agent produces the diff; this is the same thing without the agent.
    #[command(name = "g2", alias = "g2.5", alias = "g25", alias = "g3")]
    G2 {
        slug: Option<String>,
        /// Diff against this ref instead of the branch point
        #[arg(long)]
        base: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// G4 — has this run been learned from?
    G4 {
        run: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum StoreCmd {
    /// Render the store into CLAUDE.md, AGENTS.md and the rest
    Render {
        /// Show what would be written without writing it
        #[arg(long)]
        dry_run: bool,
        /// Render only this adapter
        #[arg(long)]
        adapter: Option<String>,
    },
    /// Report drift, staleness and budget breaches (exit 1 if any)
    Check {
        #[arg(long)]
        json: bool,
    },
    /// Capture hand-edits out of generated files and restore the projection
    Reconcile {
        /// Adapter ids or output paths; defaults to everything drifted
        targets: Vec<String>,
    },
}

#[derive(Subcommand)]
enum HookCmd {
    /// Install the pre-commit hook
    Install,
    /// Remove the pre-commit hook
    Uninstall,
}

fn main() {
    restore_sigpipe();
    let code = match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("keel: {e:#}");
            2
        }
    };
    std::process::exit(code);
}

/// Rust ignores `SIGPIPE`, so writing to a closed pipe returns an error and the
/// default handler panics. For a CLI that is wrong: `keel status | head` is
/// ordinary usage and must exit quietly, not print a panic.
fn restore_sigpipe() {
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init { force, yes } => {
            cmd::init::run(force, yes)?;
            Ok(0)
        }
        Command::Map { budget, full, json } => {
            cmd::map::run(budget, full, json)?;
            Ok(0)
        }
        Command::Store(StoreCmd::Render { dry_run, adapter }) => {
            cmd::store::render(dry_run, adapter)?;
            Ok(0)
        }
        Command::Store(StoreCmd::Check { json }) => cmd::store::check(json),
        Command::Store(StoreCmd::Reconcile { targets }) => {
            cmd::store::reconcile(targets)?;
            Ok(0)
        }
        Command::Status => cmd::status::run(),
        Command::Next { slug } => cmd::next::run(slug),
        Command::Doctor { json } => cmd::doctor::run(json),
        Command::Metrics { threshold, json } => cmd::metrics::run(threshold, json),
        Command::Hook(HookCmd::Install) => {
            cmd::hook::install()?;
            Ok(0)
        }
        Command::Hook(HookCmd::Uninstall) => {
            cmd::hook::uninstall()?;
            Ok(0)
        }
        Command::Spec(SpecCmd::New { slug, title, scope, force }) => {
            cmd::spec::new(&slug, title, scope, force)
        }
        Command::Spec(SpecCmd::List) => cmd::spec::list(),
        Command::Spec(SpecCmd::Prompt { slug }) => cmd::spec::print_prompt(&slug),
        Command::Plan { slug, depth } => cmd::plan::run(slug, depth),
        Command::Tasks { slug, json } => cmd::tasks::run(slug, json),
        Command::Gate(GateCmd::G0 { slug, json }) => cmd::gate::g0(slug, json),
        Command::Gate(GateCmd::G1 { slug, json }) => cmd::gate::g1(slug, json),
        Command::Gate(GateCmd::G2 { slug, base, json }) => cmd::run::run(cmd::run::Options {
            slug,
            task: None,
            driver: None,
            no_driver: true,
            base,
            json,
        }),
        Command::Gate(GateCmd::G4 { run, json }) => cmd::learn::g4(run, json),
        Command::Learn { run, json } => cmd::learn::learn(run, json),
        Command::Failures { json } => cmd::learn::failures(json),
        Command::Lessons { json } => cmd::learn::list(json),
        Command::Lesson(LessonCmd::Promote { index, force }) => cmd::learn::promote(index, force),
        Command::Lesson(LessonCmd::Reject { index, note }) => cmd::learn::reject(index, note),
        Command::Lesson(LessonCmd::Demote { id, reason }) => cmd::learn::demote(id, reason),
        Command::Approve { slug, stage, reject, note } => {
            cmd::approve::run(slug, stage, reject, note)
        }
        Command::Approvals { slug } => cmd::approve::show(slug),
        Command::Outline { path, json } => cmd::retrieve::outline(path, json),
        Command::Symbol { name, json } => cmd::retrieve::symbol(name, json),
        Command::Source { name, nth, justify, json } => {
            cmd::retrieve::source(name, nth, justify, json)
        }
        Command::Refs { name, json } => cmd::retrieve::refs(name, json),
        Command::Importers { path, json } => cmd::retrieve::importers(path, json),
        Command::Slice { task, slug, json } => cmd::retrieve::slice(slug, task, json),
        Command::Driver(DriverCmd::List) => cmd::driver::list(),
        Command::Driver(DriverCmd::Scaffold { force }) => cmd::driver::scaffold(force),
        Command::Driver(DriverCmd::Check { id, json }) => cmd::driver::check(id, json),
        Command::Mcp => {
            mcp::serve()?;
            Ok(0)
        }
        Command::Bench { json } => cmd::bench::run(json),
        Command::Blast { targets, symbol, depth, json } => {
            cmd::blast::run(targets, symbol, depth, json)
        }
        Command::Run { slug, task, driver, no_driver, waves, base, json } => {
            let opts = cmd::run::Options { slug, task, driver, no_driver, base, json };
            if waves { cmd::run::run_waves(opts) } else { cmd::run::run(opts) }
        }
        Command::Replay { run, json } => cmd::run::replay(run, json),
        Command::Runs { latest, prune, keep, apply } => {
            if prune {
                cmd::prune::prune(keep, apply)
            } else {
                cmd::run::list(latest)
            }
        }
        Command::Export { run, verify, out } => cmd::run::export(run, verify, out),
        Command::Ratchet { accept } => cmd::ratchet::run(accept),
    }
}
