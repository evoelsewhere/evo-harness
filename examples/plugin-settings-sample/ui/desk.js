// The Issue desk page: its accounts' states (evo.auth.status — names only,
// never a token), a Sign in button for each (evo.auth.signIn), the issues
// the backend reads (evo.backend.call), and an AI summary of one issue by
// the person's own models (evo.ai — docs/features/plugin-ai/spec.md): a
// thread per issue, the backend's `issue_detail` tool for the model.
const accounts = document.getElementById("accounts");
const issues = document.getElementById("issues");
const summary = document.getElementById("summary");
const error = document.getElementById("error");

function el(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
}

async function showAccounts() {
  const list = await evo.auth.status();
  accounts.replaceChildren();
  for (const account of list) {
    const who = account.signIns.find((s) => s.default) ?? account.signIns[0];
    const state = account.state === "signedIn" ? `Signed in as ${who?.label ?? "you"}` : account.state === "needsSignIn" ? "Needs signing in again" : "Not signed in";
    const verb = account.kind === "web" ? "Log in" : "Sign in";
    const button = el("button", {
      textContent: account.state === "signedIn" ? "Switch" : verb,
      className: account.state === "signedIn" ? "" : "primary",
      onclick: async () => {
        button.disabled = true;
        button.textContent = "Waiting…";
        try {
          const result = await evo.auth.signIn(account.id);
          if (result === "cancelled") evo.ui.notify(`${account.service}: sign-in cancelled`);
        } catch (e) {
          error.textContent = String(e.message ?? e);
        }
        showAccounts();
      },
    });
    const dot = el("span", { className: `dot ${account.state === "signedIn" ? "on" : account.state === "needsSignIn" ? "again" : ""}` });
    accounts.append(el("div", { className: "row" }, dot, el("span", { className: "grow" }, el("b", { textContent: account.service }), " — ", state), button));
  }
}

async function load() {
  error.textContent = "";
  summary.textContent = "Loading…";
  issues.replaceChildren();
  try {
    const result = await evo.backend.call("my_issues", {});
    const text = result.content?.[0]?.text ?? "";
    if (result.isError) throw new Error(text);
    const list = JSON.parse(text);
    summary.textContent = list.length ? `${list.length} issues` : "Nothing assigned to you.";
    for (const issue of list) {
      const summarise = el("button", { textContent: "Summarise", onclick: () => ask(`${issue.repo}#${issue.number}`) });
      issues.append(
        el("div", { className: "issue" },
          el("span", { className: "grow" }, el("div", { className: "repo", textContent: `${issue.repo} #${issue.number} · ${issue.state}` }), el("div", { textContent: issue.title })),
          summarise,
        ),
      );
    }
  } catch (e) {
    summary.textContent = "Could not load.";
    error.textContent = String(e.message ?? e);
  }
  showAccounts();
}

// ── AI summary ──────────────────────────────────────────────────────────────
const ref = document.getElementById("issue-ref");
const ai = document.getElementById("ai");
const aiState = document.getElementById("ai-state");
const aiText = document.getElementById("ai-text");
const question = document.getElementById("ai-question");
const stop = document.getElementById("stop");
let current = null;
let running = null;

const SYSTEM =
  "You help with one GitHub issue. Read it with the issue_detail tool before answering. " +
  "Be short: a few sentences or a short list. Say what is asked, what was tried, and what is open. " +
  "Write plain text: no Markdown, since the page shows your words as they are.";

/** Summarise `issue` ("owner/name#number"), or ask `text` about it, in its thread. */
async function ask(issue, text) {
  const match = /^([\w.-]+\/[\w.-]+)#(\d+)$/.exec(issue.trim());
  ai.hidden = false;
  if (!match) {
    aiState.textContent = "Name an issue as owner/name#number.";
    return;
  }
  const [, repo, number] = match;
  const id = `${repo}#${number}`;
  if (current !== id) {
    current = id;
    ref.value = id;
    aiText.replaceChildren();
  }
  if (text) aiText.append(el("div", { className: "turn", textContent: `You: ${text}` }));
  const answer = el("div");
  aiText.append(answer);
  let raw = "";
  aiState.textContent = "Choosing a model…";
  stop.hidden = false;
  try {
    running = evo.ai.run({
      need: { tier: "fast", tools: true },
      thread: id,
      system: SYSTEM,
      backendTools: ["issue_detail"],
      budget: { maxCostUsd: 0.05 },
      maxSteps: 4,
      messages: [{ role: "user", content: text ?? `Summarise issue ${id}.` }],
    });
    for await (const event of running) {
      if (event.type === "started") aiState.textContent = `${event.model} is reading ${id}…`;
      else if (event.type === "tool_call") aiState.textContent = `Reading ${event.input.repo}#${event.input.number}…`;
      else if (event.type === "text") {
        // Shown as text, never HTML; bold markers a model adds anyway go.
        raw += event.text;
        answer.textContent = raw.replace(/\*\*(.+?)\*\*/g, "$1");
      }
    }
    const result = await running.result;
    const why = { steps: " · stopped at its step limit", budget: " · stopped at its budget", cancel: " · stopped" }[result.stopped] ?? "";
    aiState.textContent = `${result.model} · $${result.usage.costUsd.toFixed(4)}${why}`;
  } catch (e) {
    // An EvoError: `budget`, `busy`, `permission`, or the run's own error.
    aiState.textContent = String(e.message ?? e);
  } finally {
    running = null;
    stop.hidden = true;
  }
}

document.getElementById("summarise").onclick = () => ask(ref.value);
ref.onkeydown = (e) => e.key === "Enter" && ask(ref.value);
document.getElementById("ask").onclick = () => {
  const text = question.value.trim();
  if (current && text) ask(current, text);
  question.value = "";
};
question.onkeydown = (e) => e.key === "Enter" && document.getElementById("ask").click();
stop.onclick = () => running?.cancel();

document.getElementById("load").onclick = load;
showAccounts().catch((e) => (accounts.textContent = String(e.message ?? e)));
