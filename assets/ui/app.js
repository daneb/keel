// keel serve — operator view.
//
// One rule governs this file: nothing that came off disk is ever assigned to
// innerHTML. Evidence bytes, check details and spec slugs are all attacker-
// influenced under keel's own threat model (a driver writes them), and this
// origin can read the whole .keel/ tree. Everything below builds nodes and sets
// textContent. There is a test that greps this file for innerHTML.

"use strict";

const STAGES = [
  ["spec", "spec"],
  ["spec_approval", "approve spec"],
  ["plan", "plan"],
  ["plan_gate", "G1"],
  ["plan_approval", "approve plan"],
  ["run", "run"],
  ["merge_approval", "approve merge"],
  ["complete", "done"],
];

let state = { specs: [], selected: null, tab: "checks", version: null };

function el(tag, cls, text) {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined && text !== null) n.textContent = String(text);
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

function renderSpine() {
  const spine = document.getElementById("spine");
  clear(spine);
  const spec = current();
  if (!spec) return;

  const at = STAGES.findIndex(([k]) => k === spec.stage);
  STAGES.forEach(([key, label], i) => {
    const node = el("div", "node", label);
    // A gate that actually failed is worth more than "you are here".
    const failed =
      (key === "spec" && gateVerdict(spec, "G0") === "fail") ||
      (key === "plan_gate" && gateVerdict(spec, "G1") === "fail");
    if (failed) node.classList.add("bad");
    else if (i < at) node.classList.add("done");
    if (i === at) node.classList.add("here");
    spine.appendChild(node);
  });

  document.getElementById("stage-line").textContent = spec.slug + " · " + stageLabel(spec.stage);
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

function render() {
  renderRail();
  renderSpine();
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
    const report = await getJSON("/api/overview");
    banner(null);
    state.specs = report.specs || [];
    if (!state.specs.some((s) => s.slug === state.selected)) {
      state.selected = state.specs.length ? state.specs[0].slug : null;
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
