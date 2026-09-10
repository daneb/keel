// keel serve — operator view.
//
// One rule governs this file: nothing that came off disk is ever assigned to
// innerHTML. Evidence bytes, check details and spec slugs are all attacker-
// influenced under keel's own threat model (a driver writes them), and this
// origin can read the whole .keel/ tree. Everything below builds nodes and sets
// textContent. There is a test that greps this file for innerHTML.

"use strict";

// Only `spec` and `plan_gate` are backed by exactly one named gate (G0, G1
// respectively) — `run` spans G2/G2.5/G3, and the rest are approvals or plain
// stages, so there is no single gate id to append to those.
const STAGES = [
  ["spec", "spec (G0)"],
  ["spec_approval", "approve spec"],
  ["plan", "plan"],
  ["plan_gate", "plan (G1)"],
  ["plan_approval", "approve plan"],
  ["run", "run"],
  ["merge_approval", "approve merge"],
  ["complete", "done"],
];

// `selected === null` means the Overview landing view, not "no spec loaded
// yet" — that state is represented by `specs` being empty.
// `timelineOpen` is the stage key whose detail is expanded below the spine,
// or `null` when none is — one panel, not one per node, so opening a second
// closes the first automatically.
let state = {
  specs: [],
  insights: null,
  selected: null,
  tab: "checks",
  version: null,
  sort: null,
  timelineOpen: null,
};

// Which gate (G0/G1) and which `approval::STAGES` entry back each stage's
// detail, when either applies. A stage absent from a map has no gate or
// approval of its own — `stageDetail` falls back to its node's own status.
const STAGE_GATE = { spec: "G0", plan_gate: "G1" };
const STAGE_APPROVAL = { spec_approval: "spec", plan_approval: "plan", merge_approval: "merge" };

function el(tag, cls, text) {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined && text !== null) n.textContent = String(text);
  return n;
}

const SVG_NS = "http://www.w3.org/2000/svg";

function svg(tag, attrs) {
  const n = document.createElementNS(SVG_NS, tag);
  for (const k in attrs || {}) n.setAttribute(k, attrs[k]);
  return n;
}

function clear(node) {
  while (node.firstChild) node.removeChild(node.firstChild);
}

function verdictGlyph(verdict) {
  const v = (verdict || "").toLowerCase();
  const word = v === "pass" ? "pass" : v === "fail" ? "FAIL" : v === "blocked" ? "BLOCKED" : "…";
  return el("span", "glyph " + (v || "muted"), word);
}

async function getJSON(url) {
  const r = await fetch(url, { headers: { Accept: "application/json" } });
  if (!r.ok) throw new Error(await r.text());
  return r.json();
}

// --- rendering -------------------------------------------------------------

function renderRail() {
  const rail = document.getElementById("rail");
  clear(rail);

  const overviewRow = el("button", "spec-row rail-overview");
  overviewRow.type = "button";
  overviewRow.setAttribute("aria-current", String(state.selected === null));
  overviewRow.appendChild(el("span", "slug", "Overview"));
  overviewRow.appendChild(el("span", "stage", "every spec"));
  overviewRow.addEventListener("click", () => {
    state.selected = null;
    render();
  });
  rail.appendChild(overviewRow);

  if (!state.specs.length) {
    rail.appendChild(el("div", "empty", "  no specs yet"));
    return;
  }
  for (const spec of state.specs) {
    const b = el("button", "spec-row");
    b.type = "button";
    b.setAttribute("aria-current", String(spec.slug === state.selected));
    b.appendChild(el("span", "slug", spec.slug));
    b.appendChild(el("span", "stage", stageLabel(spec.stage)));
    b.addEventListener("click", () => {
      if (state.selected !== spec.slug) state.timelineOpen = null;
      state.selected = spec.slug;
      render();
    });
    rail.appendChild(b);
  }
}

function stageLabel(key) {
  const found = STAGES.find(([k]) => k === key);
  return found ? found[1] : key;
}

function current() {
  return state.specs.find((s) => s.slug === state.selected) || null;
}

function gateVerdict(spec, gate) {
  const g = (spec.gates || []).find((g) => g.gate === gate);
  return g ? g.verdict : null;
}

function gateFor(spec, gate) {
  return (spec.gates || []).find((g) => g.gate === gate) || null;
}

function checkCounts(gateResult) {
  const counts = { pass: 0, fail: 0, blocked: 0 };
  for (const c of (gateResult && gateResult.checks) || []) {
    const v = (c.verdict || "").toLowerCase();
    if (Object.prototype.hasOwnProperty.call(counts, v)) counts[v] += 1;
  }
  return counts;
}

function approvalStanding(spec, stage) {
  return (spec.approvals || {})[stage] || { state: "absent" };
}

// True when the merge approval still reads `current` (its artefact hash is
// unchanged) but the pipeline has since moved back before `Complete` — the
// class of situation `lock-completed-specs` (SPEC-0005) now prevents going
// forward, but a spec approved before that fix can still be sitting in front
// of a later regression, with its approval history none the wiser.
function stale(spec) {
  return spec.stage !== "complete" && approvalStanding(spec, "merge").state === "current";
}

function staleWarning(spec, mergeApproval) {
  return el(
    "div",
    "timeline-warning",
    "approved as complete on " +
      mergeApproval.at +
      ", but the pipeline has since moved back to " +
      stageLabel(spec.stage) +
      " — a later run regressed, or this was approved against the wrong spec"
  );
}

// The detail for one stage's node, as DOM nodes for `#timeline-detail`. A
// gate stage reads its verdict and check counts from `spec.gates`; an
// approval stage reads its standing from `spec.approvals`; the remaining
// stages (`plan`, `run`, `complete`) have no gate or approval of their own,
// so they fall back to what the run history or the node's own place in the
// sequence already says.
function stageDetail(spec, key) {
  const out = [];
  const gateName = STAGE_GATE[key];
  const approvalStage = STAGE_APPROVAL[key];

  if (gateName) {
    const g = gateFor(spec, gateName);
    if (!g) {
      out.push(el("div", "muted", gateName + " has not run yet"));
    } else {
      const line = el("div", "timeline-line");
      line.appendChild(verdictGlyph(g.verdict));
      line.appendChild(el("span", null, gateName));
      out.push(line);
      const counts = checkCounts(g);
      out.push(
        el(
          "div",
          "muted",
          counts.pass + " passed · " + counts.fail + " failed · " + counts.blocked + " blocked"
        )
      );
    }
  } else if (approvalStage) {
    const a = approvalStanding(spec, approvalStage);
    if (approvalStage === "merge" && stale(spec)) out.push(staleWarning(spec, a));
    if (a.state === "current") {
      out.push(el("div", null, "approved by " + a.by));
      out.push(el("div", "muted", a.at));
    } else if (a.state === "rejected") {
      out.push(el("div", null, "rejected by " + a.by));
      if (a.note) out.push(el("div", "muted", a.note));
    } else if (a.state === "superseded") {
      out.push(
        el("div", null, "superseded — approved " + a.approved_hash + ", now " + a.current_hash)
      );
    } else {
      out.push(el("div", "muted", "not approved yet"));
    }
  } else if (key === "run") {
    const runs = spec.runs || [];
    if (!runs.length) {
      out.push(el("div", "muted", "no runs yet"));
    } else {
      const latest = runs[runs.length - 1];
      const g2 = (latest.gates || []).find((g) => g.gate === "G2");
      const line = el("div", "timeline-line");
      line.appendChild(verdictGlyph(g2 ? g2.verdict : null));
      line.appendChild(el("span", null, latest.id));
      out.push(line);
    }
  } else if (key === "complete") {
    const a = approvalStanding(spec, "merge");
    if (stale(spec)) {
      out.push(staleWarning(spec, a));
    } else {
      out.push(el("div", "muted", a.state === "current" ? "completed " + a.at : "not complete yet"));
    }
  } else {
    out.push(el("div", "muted", "created once `keel plan` has run"));
  }
  return out;
}

// The single detail panel below the spine — at most one stage's detail is
// shown at a time, so opening a second node replaces rather than stacks.
function renderTimelineDetail(spec) {
  const panel = document.getElementById("timeline-detail");
  clear(panel);
  if (!state.timelineOpen) {
    panel.hidden = true;
    return;
  }
  panel.hidden = false;
  panel.appendChild(el("div", "timeline-detail-head", stageLabel(state.timelineOpen)));
  for (const node of stageDetail(spec, state.timelineOpen)) panel.appendChild(node);
}

function renderSpine() {
  const spine = document.getElementById("spine");
  clear(spine);
  const spec = current();
  if (!spec) return;

  const at = STAGES.findIndex(([k]) => k === spec.stage);
  STAGES.forEach(([key, label], i) => {
    const node = el("button", "node", label);
    node.type = "button";
    node.dataset.stage = key;
    // A gate that actually failed is worth more than "you are here".
    const failed =
      (key === "spec" && gateVerdict(spec, "G0") === "fail") ||
      (key === "plan_gate" && gateVerdict(spec, "G1") === "fail");
    if (failed) node.classList.add("bad");
    else if (i < at) node.classList.add("done");
    if (i === at) node.classList.add("here");
    // A merge approval that still reads current but the pipeline has since
    // left `Complete` — flagged on both nodes it could mislead about.
    if ((key === "merge_approval" || key === "complete") && stale(spec)) {
      node.classList.add("stale");
    }
    node.setAttribute("aria-expanded", String(state.timelineOpen === key));
    node.addEventListener("click", () => {
      state.timelineOpen = state.timelineOpen === key ? null : key;
      for (const n of spine.querySelectorAll(".node")) {
        n.setAttribute("aria-expanded", String(state.timelineOpen === n.dataset.stage));
      }
      renderTimelineDetail(spec);
    });
    spine.appendChild(node);
  });

  renderTimelineDetail(spec);
  document.getElementById("stage-line").textContent = spec.slug + " · " + stageLabel(spec.stage);
}

// Aggregates across a spec's whole history: tokens and events sum every run,
// but fails counts only the *current* picture — `spec.gates` (G0/G1, which
// the spec directory holds one of, not one per run) plus the latest run's
// own gates — so a spec that is passing now doesn't read as failing because
// an earlier attempt once did. `gates` maps each distinct gate id this spec
// has ever produced a result for to its latest verdict: `spec.gates` first,
// then every run's `gates` in their existing oldest-first order, so the last
// write for a given id is the latest one.
function specSummary(spec) {
  const runs = spec.runs || [];
  let tokens = 0;
  let events = 0;
  for (const r of runs) {
    tokens += r.tokens || 0;
    events += r.events || 0;
  }

  const gates = new Map();
  for (const g of spec.gates || []) gates.set(g.gate, g.verdict);
  for (const r of runs) {
    for (const g of r.gates || []) gates.set(g.gate, g.verdict);
  }

  const countFails = (gs) =>
    (gs || []).reduce(
      (n, g) => n + (g.checks || []).filter((c) => (c.verdict || "").toLowerCase() !== "pass").length,
      0
    );
  const latestRun = runs.length ? runs[runs.length - 1] : null;
  const fails = countFails(spec.gates) + (latestRun ? countFails(latestRun.gates) : 0);

  return { runsTotal: runs.length, tokens, events, fails, gates };
}

function renderSpecSummary() {
  const root = document.getElementById("spec-summary");
  clear(root);
  const spec = current();
  if (!spec) return;

  const s = specSummary(spec);
  const sec = el("div", "panel-section");
  sec.appendChild(el("h2", null, "Summary"));

  const tiles = el("div", "tiles");
  tiles.appendChild(tile(s.runsTotal, "runs"));
  tiles.appendChild(tile(compactNumber(s.tokens), "tokens"));
  tiles.appendChild(tile(compactNumber(s.events), "events"));
  tiles.appendChild(tile(s.fails, "open fails", null, s.fails > 0));
  sec.appendChild(tiles);

  if (s.gates.size) {
    const row = el("div", "gate-badges");
    for (const [gate, verdict] of s.gates) {
      const b = el("span", "gate-badge " + (verdict || "").toLowerCase(), gate);
      row.appendChild(b);
    }
    sec.appendChild(row);
  }

  root.appendChild(sec);
}

function renderCheck(check, gate) {
  const v = (check.verdict || "").toLowerCase();
  const row = el("div", "check " + v);

  const head = el("div", "check-head");
  if (gate) head.appendChild(el("span", "muted", gate));
  head.appendChild(verdictGlyph(check.verdict));
  head.appendChild(el("span", "id", check.id));
  if (check.from) head.appendChild(el("span", "from", "[" + check.from + "]"));
  row.appendChild(head);

  if (check.expected || check.actual) {
    const dl = el("dl", "kv");
    dl.appendChild(el("dt", null, "expected"));
    dl.appendChild(el("dd", null, check.expected || ""));
    dl.appendChild(el("dt", null, "actual"));
    dl.appendChild(el("dd", null, check.actual || ""));
    row.appendChild(dl);
  } else if (check.detail) {
    row.appendChild(el("div", "muted", check.detail));
  }
  return row;
}

/// Failures first — that is the question an operator actually has.
function failing(gates) {
  const out = [];
  for (const g of gates || []) {
    for (const c of g.checks || []) {
      if ((c.verdict || "").toLowerCase() !== "pass") out.push([g.gate, c]);
    }
  }
  return out;
}

function renderChecks() {
  const panel = document.getElementById("panel-checks");
  clear(panel);
  const spec = current();
  if (!spec) return;

  const specFailures = failing(spec.gates);
  if (specFailures.length) {
    const head = el("div", "run-head");
    head.appendChild(el("span", "id", "spec gates"));
    panel.appendChild(head);
    for (const [gate, c] of specFailures) {
      panel.appendChild(renderCheck(c, gate));
    }
  }

  const runs = (spec.runs || []).slice().reverse();
  if (!runs.length && !specFailures.length) {
    panel.appendChild(el("div", "empty", "Nothing has failed. No runs yet for this spec."));
    return;
  }

  for (const run of runs) {
    const box = el("div", "run");
    const head = el("div", "run-head");
    head.appendChild(el("span", "id", run.id));
    head.appendChild(verdictGlyph(run.verdict));
    head.appendChild(el("span", "muted", run.events + " events · " + run.tokens + " tokens"));
    box.appendChild(head);

    if (run.anomalies && run.anomalies.length) {
      const lines = run.anomalies
        .map((a) => a.anomaly + " at line " + a.line)
        .join(", ");
      box.appendChild(
        el("div", "banner", run.anomalies.length + " trajectory record(s) unreadable: " + lines)
      );
    }

    const fails = failing(run.gates);
    if (!fails.length) {
      box.appendChild(el("div", "muted", "every check passed"));
    } else {
      for (const [gate, c] of fails) {
        box.appendChild(renderCheck(c, gate));
      }
    }
    panel.appendChild(box);
  }
}

async function renderEvidence() {
  const panel = document.getElementById("panel-evidence");
  clear(panel);
  const spec = current();
  if (!spec || !(spec.runs || []).length) {
    panel.appendChild(el("div", "empty", "No runs yet, so there is no evidence."));
    return;
  }

  for (const run of spec.runs.slice().reverse()) {
    const box = el("div", "run");
    const head = el("div", "run-head");
    head.appendChild(el("span", "id", run.id));
    head.appendChild(verdictGlyph(run.verdict));
    box.appendChild(head);
    panel.appendChild(box);

    let detail;
    try {
      detail = await getJSON("/api/run/" + encodeURIComponent(run.id));
    } catch (e) {
      box.appendChild(el("div", "muted", "could not read this run"));
      continue;
    }
    const files = detail.evidence || [];
    if (!files.length) {
      box.appendChild(el("div", "muted", "no evidence files"));
      continue;
    }
    for (const f of files) {
      const b = el("button", "file");
      b.type = "button";
      b.appendChild(el("span", "name", f.name));
      b.appendChild(el("span", "muted", humanBytes(f.bytes)));
      const pre = el("pre");
      pre.hidden = true;
      b.addEventListener("click", async () => {
        if (!pre.hidden) {
          pre.hidden = true;
          return;
        }
        pre.textContent = "…";
        pre.hidden = false;
        try {
          const url =
            "/api/run/" + encodeURIComponent(run.id) + "/evidence/" + encodeURIComponent(f.name);
          const r = await fetch(url);
          const body = await r.text();
          const total = r.headers.get("X-Keel-Total-Bytes");
          const truncated = r.headers.get("X-Keel-Truncated") === "true";
          pre.textContent = truncated
            ? "… showing the last 2 MiB of " + humanBytes(Number(total)) + "\n\n" + body
            : body;
        } catch (e) {
          pre.textContent = "could not read this file";
        }
      });
      box.appendChild(b);
      box.appendChild(pre);
    }
  }
}

function humanBytes(n) {
  if (!Number.isFinite(n)) return "";
  if (n < 1024) return n + " B";
  if (n < 1024 * 1024) return (n / 1024).toFixed(1) + " KiB";
  return (n / 1024 / 1024).toFixed(1) + " MiB";
}

// --- overview: stat tiles, trend, rankings, tables ---------------------------
//
// Color follows the entity, not its position in a sorted list: every code
// keel's taxonomy can produce has one fixed slot here, so a class or
// attribution keeps its color across refreshes even as the ranking around it
// moves. Ten failure classes share eight series slots by design — see
// src/failure/taxonomy.rs for the full set.

const ATTRIBUTION_COLOR = {
  AGENTIC: "var(--series-1)",
  PROCESS: "var(--series-2)",
  HUMAN: "var(--series-3)",
  UNATTRIBUTABLE: "var(--series-8)",
};

const CLASS_COLOR = {
  "SPEC-AMBIG": "var(--series-1)",
  "SPEC-MISSING": "var(--series-2)",
  "LOC-WRONG": "var(--series-3)",
  "CTX-STALE": "var(--series-4)",
  "CTX-DRIFT": "var(--series-5)",
  "EDIT-COMPILE": "var(--series-6)",
  "EDIT-RUNTIME": "var(--series-7)",
  "TEST-INVALID": "var(--series-8)",
  "SCOPE-CREEP": "var(--series-1)",
  "CONV-VIOLATION": "var(--series-2)",
};

function showTip(evt, text) {
  const tip = document.getElementById("tooltip");
  tip.textContent = text;
  tip.style.left = evt.clientX + "px";
  tip.style.top = evt.clientY + "px";
  tip.hidden = false;
}
function hideTip() {
  document.getElementById("tooltip").hidden = true;
}

// --- glossary: info overlays ------------------------------------------------
//
// Definitions for terms an operator meets on this page but that are only
// documented in Rust doc comments (src/failure/taxonomy.rs, src/report/
// insights.rs) or PLAN.md. One source of truth here, reused by every `?`
// button below rather than re-explained inline at each call site.

const GLOSSARY = {
  "specs complete": "Specs whose merge approval is current and the pipeline has reached `complete`.",
  "run pass rate": "Share of runs whose G2 gate passed, across every spec.",
  "tokens total": "Tokens spent across every run this store has recorded.",
  "tokens this week": "Tokens spent by runs started in the last 7 days.",
  "human decisions": "Approvals and rejections a person has recorded (spec, plan, merge). Highlighted when a run is currently waiting on one.",
  "lessons in force": "Promoted lessons still inside their decay window — see the Lessons table below.",
  "gate(s) never fail":
    "Checks that have run at least 20 times and never once failed or blocked — \"theatre\": either the check is trivially satisfied or it is not wired to anything that can actually break. Flagged per-check in the table below as \"theatre?\".",
  "Trend, by week": "Run outcomes and token spend, bucketed by the week a run started.",
  "Failure attribution":
    "Every non-pass check, classified by whose failure it was — see the definitions on AGENTIC, PROCESS, HUMAN and UNATTRIBUTABLE below. Attribution comes first: only AGENTIC failures ever become lessons, so this chart is what keeps the taxonomy honest.",
  "harness-fixable":
    "Of AGENTIC failures, the share whose class (SCOPE-CREEP, CONV-VIOLATION, SPEC-AMBIG, SPEC-MISSING, LOC-WRONG, CTX-STALE, TEST-INVALID) is something a gate could plausibly catch — as opposed to EDIT-COMPILE/EDIT-RUNTIME/CTX-DRIFT, which mostly measure the model itself.",
  "failure classes": "The locus of each AGENTIC failure — which part of the pipeline it traces back to. Hover a bar for its definition.",
  "Checks, worst pass rate first": "Every check any gate has run, ranked by how often it fails or blocks. A check with a 100% pass rate over many runs is worth a look — see \"theatre?\" below.",
  "theatre?": "This check has run at least 20 times and never failed or blocked. It may be redundant, or not actually wired to catch the thing it claims to.",
  Specs: "Every spec this store knows about, with its current stage and running totals.",
  "cycle time": "Days from the spec's first run to its merge approval.",
  Lessons: "Lessons promoted from AGENTIC failures — the only attribution that can produce one. Each fires again if the same failure class recurs.",
  enforced: "This lesson has an oracle attached (a check that can catch a repeat) rather than being advisory-only text a reviewer has to remember.",
  advisory: "This lesson has no oracle — it relies on a reviewer remembering it, rather than a check catching a repeat.",
  "past decay": "This lesson has gone idle longer than its decay window — it has not fired recently enough to justify still gating on it. Consider retiring or re-verifying it.",

  AGENTIC: "An observable technical failure caused by the agent's own output.",
  PROCESS: "Workflow or environment: flaky infra, a missing tool, a run superseded by a later one.",
  HUMAN: "A person changed their mind, redirected, or renegotiated scope — not a defect.",
  UNATTRIBUTABLE: "No observable rationale for the failure. Counted for honesty, but never promoted into a lesson.",

  "SPEC-AMBIG": "The spec was ambiguous — caught by G0's ambiguity check, or a late \"what did you mean\".",
  "SPEC-MISSING": "An acceptance criterion was missing and had to be added mid-implementation.",
  "LOC-WRONG": "The agent edited the wrong file, or a symbol outside the declared blast radius.",
  "CTX-STALE": "The agent acted on a store or map entry that was older than the code it described.",
  "CTX-DRIFT": "The agent contradicted a fact it had already established earlier in the same run.",
  "EDIT-COMPILE": "The change didn't build.",
  "EDIT-RUNTIME": "A test failed, an assertion failed, or the change threw at runtime.",
  "TEST-INVALID": "Tests passed, but only because they mock away the behaviour under test.",
  "SCOPE-CREEP": "The diff exceeded the blast radius or budget the spec declared.",
  "CONV-VIOLATION": "A lint or house-rule breach: naming, layering, or another local convention.",
  OTHER: "Every class or category past the top ranked ones, folded together so the chart stays legible.",
};

/// A small `?` button that shows a glossary definition on hover/focus and
/// toggles a pinned tooltip on click/tap, for operators without a mouse.
/// `key` looks up `GLOSSARY`; falls back to `label` when the two differ (e.g.
/// a ranked-bar row keyed by its own code).
function infoIcon(key) {
  const text = GLOSSARY[key];
  if (!text) return null;
  const btn = el("button", "info-icon", "?");
  btn.type = "button";
  btn.setAttribute("aria-label", key + ": " + text);
  let pinned = false;
  btn.addEventListener("mouseenter", (e) => showTip(e, text));
  btn.addEventListener("mousemove", (e) => {
    if (!pinned) showTip(e, text);
  });
  btn.addEventListener("mouseleave", () => {
    if (!pinned) hideTip();
  });
  btn.addEventListener("focus", (e) => showTip(e, text));
  btn.addEventListener("blur", () => {
    pinned = false;
    hideTip();
  });
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    pinned = !pinned;
    if (pinned) showTip(e, text);
    else hideTip();
  });
  return btn;
}

/// A heading or label plus its `?` info icon, when the glossary has an
/// entry for it — otherwise just the plain text node, unchanged.
function withInfo(tag, cls, text, glossaryKey) {
  const node = el(tag, cls, text);
  const icon = infoIcon(glossaryKey || text);
  if (icon) node.appendChild(icon);
  return node;
}

/// Like `infoIcon`, but for elements too narrow to fit a separate `?` button
/// (the ranked-bar label column) — the element itself becomes the hover/
/// focus target, marked with a dotted underline for discoverability, rather
/// than growing to fit an appended icon.
function tipify(node, glossaryKey) {
  const text = GLOSSARY[glossaryKey];
  if (!text) return node;
  node.classList.add("has-tip");
  node.tabIndex = 0;
  node.addEventListener("mouseenter", (e) => showTip(e, text));
  node.addEventListener("mousemove", (e) => showTip(e, text));
  node.addEventListener("mouseleave", hideTip);
  node.addEventListener("focus", (e) => showTip(e, text));
  node.addEventListener("blur", hideTip);
  return node;
}

function tile(value, label, ofTotal, warn) {
  const t = el("div", "tile" + (warn ? " warn" : ""));
  const v = el("div", "v");
  v.appendChild(document.createTextNode(value));
  if (ofTotal) v.appendChild(el("span", "of", " / " + ofTotal));
  t.appendChild(v);
  t.appendChild(withInfo("div", "l", label));
  return t;
}

function statTiles(i) {
  const o = i.overview;
  const wrap = el("div", "tiles");
  wrap.appendChild(tile(o.specs_complete, "specs complete", o.specs_total));
  wrap.appendChild(tile(Math.round(o.pass_rate * 100) + "%", "run pass rate"));
  wrap.appendChild(tile(compactNumber(o.tokens_total), "tokens total"));
  wrap.appendChild(tile(compactNumber(o.tokens_this_week), "tokens this week"));
  wrap.appendChild(tile(o.human_decisions, "human decisions", null, o.runs_awaiting_human > 0));
  wrap.appendChild(tile(o.lessons_in_force, "lessons in force"));
  wrap.appendChild(tile(o.theatre_count, "gate(s) never fail", null, o.theatre_count > 0));
  return wrap;
}

function compactNumber(n) {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1000) return (n / 1000).toFixed(1) + "k";
  return String(n);
}

/// A weekly stacked bar (run outcomes, or a single-series token volume).
/// `series` is `[{key, color, get(bucket)}]`; segments stack in array order.
function weeklyBarChart(trend, series, height) {
  const w = 400,
    h = height || 120,
    padL = 4,
    padR = 4,
    padB = 16,
    padT = 6;
  const plotW = w - padL - padR;
  const plotH = h - padT - padB;
  const n = Math.max(trend.length, 1);
  const bw = Math.min(28, (plotW / n) * 0.62);
  const gap = plotW / n;

  const totals = trend.map((b) => series.reduce((s, ser) => s + ser.get(b), 0));
  const max = Math.max(1, ...totals);

  const s = svg("svg", {
    viewBox: `0 0 ${w} ${h}`,
    width: "100%",
    height: h,
    role: "img",
    "aria-label": "weekly trend",
  });
  // baseline
  s.appendChild(
    svg("line", {
      class: "chart-grid",
      x1: padL,
      x2: w - padR,
      y1: h - padB,
      y2: h - padB,
    })
  );

  trend.forEach((bucket, idx) => {
    const cx = padL + gap * idx + gap / 2;
    let y = h - padB;
    for (const ser of series) {
      const val = ser.get(bucket);
      if (val <= 0) continue;
      const segH = (val / max) * plotH;
      y -= segH;
      const rect = svg("rect", {
        class: "bar-mark",
        x: cx - bw / 2,
        y: y,
        width: bw,
        height: Math.max(segH, 0.5),
        fill: ser.color,
        rx: 1.5,
      });
      rect.addEventListener("mousemove", (e) => showTip(e, `${bucket.week_start} — ${ser.key}: ${val}`));
      rect.addEventListener("mouseleave", hideTip);
      s.appendChild(rect);
    }
    // sparse labels: first, last, and every third bucket in between
    if (idx === 0 || idx === trend.length - 1 || idx % 3 === 0) {
      const t = svg("text", {
        class: "chart-axis-label",
        x: cx,
        y: h - 4,
        "text-anchor": "middle",
      });
      t.textContent = bucket.week_start.slice(5); // MM-DD
      s.appendChild(t);
    }
  });

  return s;
}

function legendRow(entries) {
  const wrap = el("div", "legend");
  for (const [label, color] of entries) {
    const item = el("span");
    const sw = document.createElement("span");
    sw.className = "sw";
    sw.style.background = color;
    item.appendChild(sw);
    item.appendChild(document.createTextNode(label));
    wrap.appendChild(item);
  }
  return wrap;
}

function trendSection(i) {
  const sec = el("div", "panel-section");
  sec.appendChild(withInfo("h2", null, "Trend, by week"));
  if (!i.trend.length) {
    sec.appendChild(el("div", "empty", "Not enough run history yet for a trend."));
    return sec;
  }
  const row = el("div", "chart-row");

  const runsBox = el("div", "chart-box");
  runsBox.appendChild(el("div", "cap", "runs per week, by outcome"));
  runsBox.appendChild(
    weeklyBarChart(i.trend, [
      { key: "passed", color: "var(--pass)", get: (b) => b.passed },
      { key: "failed", color: "var(--fail)", get: (b) => b.failed },
      { key: "blocked", color: "var(--blocked)", get: (b) => b.blocked },
    ])
  );
  runsBox.appendChild(
    legendRow([
      ["passed", "var(--pass)"],
      ["failed", "var(--fail)"],
      ["blocked", "var(--blocked)"],
    ])
  );
  row.appendChild(runsBox);

  const tokensBox = el("div", "chart-box");
  tokensBox.appendChild(el("div", "cap", "tokens per week"));
  tokensBox.appendChild(
    weeklyBarChart(i.trend, [{ key: "tokens", color: "var(--series-1)", get: (b) => b.tokens }])
  );
  row.appendChild(tokensBox);

  sec.appendChild(row);
  return sec;
}

/// A ranked horizontal bar list. `entries` is `[[label, count], ...]`,
/// already in the order to display (caller decides ranking and any "Other"
/// folding — see the dataviz skill's categorical cap).
function rankedBars(entries, colorFor) {
  const wrap = el("div", "ranked");
  const max = Math.max(1, ...entries.map(([, n]) => n));
  for (const [label, n] of entries) {
    const row = el("div", "ranked-row");
    row.appendChild(tipify(el("div", "rlabel", label), label));
    const track = el("div", "rtrack");
    const fill = document.createElement("div");
    fill.className = "rfill";
    fill.style.width = Math.max(3, (n / max) * 100) + "%";
    fill.style.background = colorFor(label);
    track.appendChild(fill);
    row.appendChild(track);
    row.appendChild(el("div", "rcount", n));
    wrap.appendChild(row);
  }
  return wrap;
}

/// Fold everything past the top `cap` entries into "Other" — the dataviz
/// skill's categorical series cap; past eight or so a ranked list is more
/// legible collapsed than color-starved.
function capRanked(pairs, cap) {
  const sorted = pairs.slice().sort((a, b) => b[1] - a[1]);
  if (sorted.length <= cap) return sorted;
  const head = sorted.slice(0, cap);
  const rest = sorted.slice(cap).reduce((s, [, n]) => s + n, 0);
  if (rest > 0) head.push(["OTHER", rest]);
  return head;
}

function failureSection(i) {
  const sec = el("div", "panel-section");
  sec.appendChild(withInfo("h2", null, "Failure attribution"));
  if (!i.attribution.length) {
    sec.appendChild(el("div", "empty", "No failures recorded yet."));
    return sec;
  }
  const row = el("div", "chart-row");

  const attrBox = el("div", "chart-box");
  attrBox.appendChild(
    rankedBars(i.attribution, (label) => ATTRIBUTION_COLOR[label] || "var(--muted)")
  );
  const callout = el("div", "callout");
  callout.appendChild(el("span", "v", Math.round(i.harness_fixable_rate * 100) + "%"));
  callout.appendChild(withInfo("span", null, "of agentic failures look harness-fixable", "harness-fixable"));
  attrBox.appendChild(callout);
  row.appendChild(attrBox);

  const classBox = el("div", "chart-box");
  classBox.appendChild(withInfo("div", "cap", "failure classes"));
  classBox.appendChild(
    rankedBars(capRanked(i.failure_classes, 8), (label) => CLASS_COLOR[label] || "var(--muted)")
  );
  row.appendChild(classBox);

  sec.appendChild(row);
  return sec;
}

/// Column headers that sort `rows` in place and re-render on click. `cols` is
/// `[{key, label, num, get(row)}]`; `get` returns the sortable value.
function sortableTable(rows, cols, rowBuilder, sortKey) {
  const table = el("table", "data");
  const thead = document.createElement("thead");
  const htr = document.createElement("tr");
  for (const c of cols) {
    const th = el("th", c.num ? "num" : null, c.label);
    if (state.sort && state.sort.table === sortKey && state.sort.col === c.key) {
      th.textContent = c.label + (state.sort.dir === 1 ? " ▲" : " ▼");
    }
    const icon = infoIcon(c.glossary || c.label);
    if (icon) th.appendChild(icon);
    th.addEventListener("click", () => {
      const cur = state.sort;
      const dir = cur && cur.table === sortKey && cur.col === c.key ? -cur.dir : -1;
      state.sort = { table: sortKey, col: c.key, dir };
      render();
    });
    htr.appendChild(th);
  }
  thead.appendChild(htr);
  table.appendChild(thead);

  let sorted = rows;
  if (state.sort && state.sort.table === sortKey) {
    const col = cols.find((c) => c.key === state.sort.col);
    if (col) {
      sorted = rows.slice().sort((a, b) => {
        const av = col.get(a),
          bv = col.get(b);
        if (av < bv) return -state.sort.dir;
        if (av > bv) return state.sort.dir;
        return 0;
      });
    }
  }

  const tbody = document.createElement("tbody");
  for (const row of sorted) tbody.appendChild(rowBuilder(row));
  table.appendChild(tbody);
  return table;
}

function checksSection(i) {
  const sec = el("div", "panel-section");
  sec.appendChild(withInfo("h2", null, "Checks, worst pass rate first"));
  if (!i.checks.length) {
    sec.appendChild(el("div", "empty", "No gates have run yet."));
    return sec;
  }
  const cols = [
    { key: "check", label: "check", get: (c) => c.gate + "/" + c.check },
    { key: "rate", label: "pass rate", num: true, get: (c) => (c.runs ? c.passed / c.runs : 0) },
    { key: "runs", label: "runs", num: true, get: (c) => c.runs },
    { key: "theatre", label: "", get: () => 0 },
  ];
  const rowBuilder = (c) => {
    const tr = document.createElement("tr");
    tr.appendChild(el("td", null, c.gate + "/" + c.check));
    const rate = c.runs ? c.passed / c.runs : 0;
    const rateTd = el("td", "num");
    const stack = document.createElement("span");
    stack.className = "mini-stack";
    for (const [n, color] of [
      [c.passed, "var(--pass)"],
      [c.failed, "var(--fail)"],
      [c.blocked, "var(--blocked)"],
    ]) {
      if (!n) continue;
      const seg = document.createElement("span");
      seg.style.width = ((n / c.runs) * 100).toFixed(1) + "%";
      seg.style.background = color;
      stack.appendChild(seg);
    }
    rateTd.appendChild(stack);
    rateTd.appendChild(document.createTextNode(" " + Math.round(rate * 100) + "%"));
    tr.appendChild(rateTd);
    tr.appendChild(el("td", "num", c.runs));
    const flagTd = document.createElement("td");
    if (c.runs >= i.theatre_threshold && c.failed === 0 && c.blocked === 0) {
      flagTd.appendChild(withInfo("span", "badge theatre", "theatre?"));
    }
    tr.appendChild(flagTd);
    return tr;
  };
  sec.appendChild(sortableTable(i.checks, cols, rowBuilder, "checks"));
  return sec;
}

function specsSection(i) {
  const sec = el("div", "panel-section");
  sec.appendChild(withInfo("h2", null, "Specs"));
  if (!i.specs.length) {
    sec.appendChild(el("div", "empty", "No specs yet."));
    return sec;
  }
  const cols = [
    { key: "slug", label: "spec", get: (s) => s.slug },
    { key: "stage", label: "stage", get: (s) => s.stage },
    { key: "rate", label: "pass rate", num: true, get: (s) => s.pass_rate },
    { key: "runs", label: "runs", num: true, get: (s) => s.runs },
    { key: "tokens", label: "tokens", num: true, get: (s) => s.tokens_total },
    { key: "cycle", label: "cycle time", num: true, get: (s) => s.cycle_time_days ?? -1 },
  ];
  const rowBuilder = (s) => {
    const tr = document.createElement("tr");
    tr.className = "clickable";
    tr.addEventListener("click", () => {
      state.selected = s.slug;
      render();
    });
    tr.appendChild(el("td", null, s.slug));
    tr.appendChild(el("td", null, stageLabel(s.stage)));
    tr.appendChild(el("td", "num", Math.round(s.pass_rate * 100) + "%"));
    tr.appendChild(el("td", "num", s.runs));
    tr.appendChild(el("td", "num", compactNumber(s.tokens_total)));
    tr.appendChild(
      el("td", "num", s.cycle_time_days != null ? s.cycle_time_days.toFixed(1) + "d" : "—")
    );
    return tr;
  };
  sec.appendChild(sortableTable(i.specs, cols, rowBuilder, "specs"));
  return sec;
}

function lessonsSection(i) {
  const sec = el("div", "panel-section");
  sec.appendChild(withInfo("h2", null, "Lessons"));
  if (!i.lessons.length) {
    sec.appendChild(el("div", "empty", "No lessons in force yet."));
    return sec;
  }
  const table = el("table", "data");
  const thead = document.createElement("thead");
  const htr = document.createElement("tr");
  for (const label of ["lesson", "class", "occurrences", "status", "idle"]) {
    htr.appendChild(el("th", null, label));
  }
  thead.appendChild(htr);
  table.appendChild(thead);
  const tbody = document.createElement("tbody");
  for (const l of i.lessons) {
    const tr = document.createElement("tr");
    tr.appendChild(el("td", null, l.id));
    tr.appendChild(el("td", null, l.class));
    tr.appendChild(el("td", "num", l.occurrences));
    const statusTd = document.createElement("td");
    statusTd.appendChild(
      withInfo("span", "badge " + (l.enforced ? "enforced" : ""), l.enforced ? "enforced" : "advisory")
    );
    tr.appendChild(statusTd);
    const stale = l.idle_days > l.decay_days;
    const idleTd = document.createElement("td");
    idleTd.appendChild(document.createTextNode(l.idle_days + "d "));
    if (stale) idleTd.appendChild(withInfo("span", "badge stale", "past decay"));
    tr.appendChild(idleTd);
    tbody.appendChild(tr);
  }
  table.appendChild(tbody);
  sec.appendChild(table);
  return sec;
}

function renderOverview() {
  const root = document.getElementById("overview");
  clear(root);
  const i = state.insights;
  if (!i) return;

  const head = el("div", "overview-head");
  head.appendChild(el("h1", null, "Overview"));
  root.appendChild(head);

  if (i.overview.specs_total === 0) {
    root.appendChild(el("div", "empty", "No specs yet — `keel spec new <slug>`"));
    return;
  }

  root.appendChild(statTiles(i));
  root.appendChild(trendSection(i));
  root.appendChild(failureSection(i));
  root.appendChild(checksSection(i));
  root.appendChild(specsSection(i));
  root.appendChild(lessonsSection(i));
}

function render() {
  renderRail();

  const overviewSection = document.getElementById("overview");
  const detailSection = document.getElementById("detail");

  if (state.selected === null) {
    overviewSection.hidden = false;
    detailSection.hidden = true;
    document.getElementById("stage-line").textContent = "overview";
    renderOverview();
    return;
  }

  overviewSection.hidden = true;
  detailSection.hidden = false;
  renderSpine();
  renderSpecSummary();
  for (const t of document.querySelectorAll(".tab")) {
    t.setAttribute("aria-selected", String(t.dataset.tab === state.tab));
  }
  document.getElementById("panel-checks").hidden = state.tab !== "checks";
  document.getElementById("panel-evidence").hidden = state.tab !== "evidence";
  if (state.tab === "checks") renderChecks();
  else renderEvidence();
}

function banner(message) {
  const b = document.getElementById("banner");
  if (!message) {
    b.hidden = true;
    return;
  }
  b.textContent = message;
  b.hidden = false;
}

async function refresh() {
  try {
    const [report, insights] = await Promise.all([getJSON("/api/overview"), getJSON("/api/insights")]);
    banner(null);
    state.specs = report.specs || [];
    state.insights = insights;
    // A spec that was deleted or renamed out from under an open tab falls
    // back to Overview rather than pointing at nothing.
    if (state.selected !== null && !state.specs.some((s) => s.slug === state.selected)) {
      state.selected = null;
    }
    render();
  } catch (e) {
    banner("Could not read .keel/ — " + e.message);
  }
}

// --- change polling --------------------------------------------------------
//
// A hidden tab polls nothing: this is a local tool an operator leaves open all
// day, and a background tab waking the filesystem twice a second is rude.

async function poll() {
  if (document.visibilityState !== "visible") return;
  try {
    const v = await getJSON("/api/version");
    document.getElementById("live").textContent = "·";
    if (v.version !== state.version) {
      state.version = v.version;
      await refresh();
    }
  } catch (e) {
    document.getElementById("live").textContent = "offline";
  }
}

for (const t of document.querySelectorAll(".tab")) {
  t.addEventListener("click", () => {
    state.tab = t.dataset.tab;
    render();
  });
}

document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") poll();
});

refresh().then(poll);
setInterval(poll, 2000);
