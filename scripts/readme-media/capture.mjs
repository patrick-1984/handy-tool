// Captures the README user stories from the real UI (vite dev on :1420 with
// the Tauri backend mocked by mock.js). Writes frames + captions per story to
// <out>/<story>/, then `python make_gifs.py <out> docs/media` builds the GIFs.
//
//   node scripts/readme-media/gen-defaults.mjs scripts/readme-media/defaults.json
//   bunx vite dev            (in the repo root, another terminal)
//   node scripts/readme-media/capture.mjs <out> [story ...]
//   python scripts/readme-media/make_gifs.py <out> docs/media   (needs Pillow)
// Uses the installed Microsoft Edge; all data is made up (data.mjs).
import { chromium } from "playwright-core";
import { mkdirSync, writeFileSync } from "fs";
import { baseHandlers, initScript } from "./data.mjs";

const BASE = "http://localhost:1420";
const OUT = process.argv[2];
const ONLY = process.argv.slice(3);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Screenshots as fast as the page allows, each with its time and caption. */
class Recorder {
  constructor(page, dir) {
    this.page = page;
    this.dir = dir;
    this.frames = [];
    this.caption = "";
    mkdirSync(dir, { recursive: true });
  }
  start() {
    this.running = true;
    this.t0 = Date.now();
    this.loop = (async () => {
      while (this.running) {
        const t = Date.now() - this.t0;
        const caption = this.caption;
        const buf = await this.page.screenshot({ animations: "allow" });
        const i = this.frames.length;
        writeFileSync(`${this.dir}/f${String(i).padStart(4, "0")}.png`, buf);
        this.frames.push({ t, caption });
      }
    })();
  }
  async stop() {
    this.running = false;
    await this.loop;
    this.frames.push({
      t: Date.now() - this.t0,
      caption: this.caption,
      end: true,
    });
    writeFileSync(`${this.dir}/frames.json`, JSON.stringify(this.frames));
  }
}

/** A mouse pointer drawn in the page, gliding to what it clicks. */
const CURSOR = `(() => {
  if (document.getElementById("rm-cursor")) return;
  const c = document.createElement("div");
  c.id = "rm-cursor";
  c.innerHTML = '<svg width="22" height="22" viewBox="0 0 24 24"><path d="M3 2l7 19 2.6-7.4L20 11z" fill="#111" stroke="#fff" stroke-width="1.6" stroke-linejoin="round"/></svg>';
  Object.assign(c.style, { position: "fixed", left: "700px", top: "500px", zIndex: 2147483647, pointerEvents: "none", transition: "left 600ms cubic-bezier(.3,.7,.3,1), top 600ms cubic-bezier(.3,.7,.3,1)" });
  document.body.appendChild(c);
  const r = document.createElement("div");
  r.id = "rm-ripple";
  Object.assign(r.style, { position: "fixed", width: "34px", height: "34px", marginLeft: "-17px", marginTop: "-17px", borderRadius: "50%", border: "3px solid rgba(41,163,200,.9)", opacity: 0, zIndex: 2147483646, pointerEvents: "none" });
  document.body.appendChild(r);
})()`;

async function pointAt(page, locator) {
  await page.evaluate(CURSOR);
  const box = await locator.boundingBox();
  const x = box.x + Math.min(box.width / 2, 60);
  const y = box.y + box.height / 2;
  await page.evaluate(
    ([x, y]) => {
      const c = document.getElementById("rm-cursor");
      c.style.left = `${x - 3}px`;
      c.style.top = `${y - 2}px`;
    },
    [x, y],
  );
  await sleep(700);
  return { x, y };
}

async function clickAt(page, locator) {
  const { x, y } = await pointAt(page, locator);
  await page.evaluate(
    ([x, y]) => {
      const r = document.getElementById("rm-ripple");
      r.style.left = `${x}px`;
      r.style.top = `${y}px`;
      r.animate(
        [
          { opacity: 0.9, transform: "scale(.4)" },
          { opacity: 0, transform: "scale(1.4)" },
        ],
        { duration: 450 },
      );
    },
    [x, y],
  );
  await locator.click();
}

async function typeInto(page, locator, text) {
  await pointAt(page, locator);
  await locator.click();
  for (const ch of text) {
    await page.keyboard.type(ch);
    await sleep(28);
  }
}

async function openApp(
  browser,
  handlers,
  fnSource = "",
  viewport = { width: 960, height: 600 },
) {
  const context = await browser.newContext({ viewport });
  await context.addInitScript({
    content: initScript(handlers, "main", fnSource),
  });
  const page = await context.newPage();
  page.on("pageerror", (e) => console.log("PAGEERR", e.message.slice(0, 200)));
  await page.goto(BASE + "/");
  await page.waitForTimeout(1500);
  return page;
}

/** Scrolls smoothly until the element is in the middle of the view. */
async function scrollTo(page, locator) {
  await locator
    .first()
    .evaluate((e) => e.scrollIntoView({ behavior: "smooth", block: "center" }));
  await sleep(900);
}

// Backend behaviour for the models story, run inside the page.
const MODEL_FNS = `{
  download_model: ({ modelId }) => {
    const H = window.__RM__.handlers;
    const total = 1100 * 1048576;
    let p = 0;
    const iv = setInterval(() => {
      p = Math.min(100, p + 6);
      window.__emit("model-download-progress", { model_id: modelId, generation: 1, downloaded: Math.round((total * p) / 100), total, percentage: p });
      if (p >= 100) {
        clearInterval(iv);
        H.get_available_models = H.get_available_models.map((m) => (m.id === modelId ? { ...m, is_downloaded: true } : m));
        window.__emit("model-download-complete", { model_id: modelId, generation: 1 });
      }
    }, 170);
    return null;
  },
  set_active_model: ({ modelId }) => {
    const H = window.__RM__.handlers;
    H.get_current_model = modelId;
    H.get_transcription_model_status = modelId;
    H.get_app_settings = { ...H.get_app_settings, selected_model: modelId };
    window.__emit("model-state-changed", { event_type: "loading_started", model_id: modelId });
    setTimeout(() => window.__emit("model-state-changed", { event_type: "loading_completed", model_id: modelId }), 900);
    return null;
  },
}`;

const PROVIDERS = [
  ["or-claude", "openrouter", "Claude Sonnet", "anthropic/claude-sonnet-4.5"],
  ["or-gpt", "openrouter", "GPT-5 mini", "openai/gpt-5-mini"],
  ["or-gemini", "openrouter", "Gemini Flash", "google/gemini-2.5-flash"],
  ["local-llm", "openai_compatible", "Local Llama", "llama3.2:3b"],
].map(([id, kind, name, model]) => ({
  id,
  kind,
  enabled: true,
  name,
  model,
  base_url: "",
  api_key: "",
}));

const TEST_FNS = `{
  run_model_test: () => new Promise((resolve) => setTimeout(() => resolve({
    round_trip_ms: 2840,
    outcomes: [
      { provider_id: "or-claude", provider_name: "Claude Sonnet", model: "anthropic/claude-sonnet-4.5", ok: true, content: "Hi team,\\n\\nCould we move the design review to Thursday afternoon? The new onboarding screens need one more pass first.\\n\\nThanks!", error: null, input_tokens: 62, output_tokens: 41, cost_usd: 0.00080, cost_is_real: true, elapsed_ms: 2310 },
      { provider_id: "or-gpt", provider_name: "GPT-5 mini", model: "openai/gpt-5-mini", ok: true, content: "Hello everyone,\\n\\nCan we shift the design review to Thursday afternoon? The onboarding screens need another pass.\\n\\nBest,", error: null, input_tokens: 60, output_tokens: 38, cost_usd: 0.00009, cost_is_real: true, elapsed_ms: 1740 },
      { provider_id: "or-gemini", provider_name: "Gemini Flash", model: "google/gemini-2.5-flash", ok: true, content: "Hi all,\\n\\nWould Thursday afternoon work for the design review? The onboarding screens need one more pass.\\n\\nCheers", error: null, input_tokens: 61, output_tokens: 35, cost_usd: 0.00012, cost_is_real: true, elapsed_ms: 1210 },
      { provider_id: "local-llm", provider_name: "Local Llama", model: "llama3.2:3b", ok: true, content: "Hi team, can we move the design review to Thursday afternoon? The onboarding screens need one more pass. Thanks!", error: null, input_tokens: 64, output_tokens: 33, cost_usd: null, cost_is_real: false, elapsed_ms: 2840 },
    ],
  }), 2600)),
}`;

const nav = (page, label) =>
  page.getByText(label, { exact: true }).first().click();

// ---------------------------------------------------------------- stories

/** Speech-like sound bars for the pill. */
async function speak(frame, ms) {
  const end = Date.now() + ms;
  let k = 0;
  while (Date.now() < end) {
    k++;
    const env =
      0.35 + 0.65 * Math.abs(Math.sin(k / 3.1)) * (k % 9 < 7 ? 1 : 0.15);
    const levels = Array.from({ length: 16 }, (_, i) =>
      Math.max(
        0,
        env *
          (0.55 + 0.45 * Math.sin(i * 0.9 + k)) *
          (0.6 + Math.random() * 0.4),
      ),
    );
    await frame.evaluate(
      (l) =>
        window.__emit("mic-level", { levels: l, live: true, too_quiet: false }),
      levels,
    );
    await sleep(60);
  }
}

async function desktop(browser, settingsOver = {}) {
  const context = await browser.newContext({
    viewport: { width: 960, height: 560 },
  });
  await context.addInitScript({
    content: initScript(
      baseHandlers({ settings: { pill_scale: 130, ...settingsOver } }),
    ),
  });
  const page = await context.newPage();
  page.on("pageerror", (e) => console.log("PAGEERR", e.message.slice(0, 200)));
  await page.goto(BASE + "/scripts/readme-media/desktop.html");
  await page.evaluate(() => {
    const f = document.getElementById("pill");
    f.width = String(Math.round(208 * 1.3));
    f.height = String(Math.round(56 * 1.3));
    f.src = "/src/overlay/index.html";
  });
  await page.waitForTimeout(2000);
  const frame = page.frames().find((f) => f.url().includes("/src/overlay/"));
  return { page, frame };
}

const keys = (page, list) =>
  page.evaluate((list) => {
    const k = document.getElementById("keys");
    k.innerHTML = list
      .map((x) =>
        x === "+" ? "<span>+</span>" : `<span class="key">${x}</span>`,
      )
      .join("");
    k.classList.toggle("on", list.length > 0);
  }, list);

async function pillTake(page, frame, rec, { speakMs = 3200, captions }) {
  rec.caption = captions[0];
  await keys(page, ["Ctrl", "+", "Space"]);
  await sleep(500);
  await frame.evaluate(() => window.__emit("show-overlay", "recording"));
  await sleep(400);
  await keys(page, []);
  await speak(frame, speakMs);
  rec.caption = captions[1];
  await keys(page, ["Ctrl", "+", "Space"]);
  await sleep(350);
  await frame.evaluate(() => window.__emit("show-overlay", "transcribing"));
  await keys(page, []);
  for (let p = 0; p <= 100; p += 9) {
    await frame.evaluate(
      (p) => window.__emit("transcription-progress", Math.min(p, 100)),
      p,
    );
    await sleep(110);
  }
  await frame.evaluate(() => window.__emit("transcription-progress", 100));
  await sleep(250);
  await frame.evaluate(() => window.__emit("hide-overlay"));
}

const STORIES = {
  /** 1. Dictate into any app: shortcut, speak, the text is pasted. */
  async dictate(browser, dir) {
    const { page, frame } = await desktop(browser);
    const rec = new Recorder(page, dir);
    rec.caption = "Click where you want the text";
    rec.start();
    await sleep(1200);
    await pillTake(page, frame, rec, {
      captions: [
        "Press Ctrl+Space and speak",
        "Press it again: Handy Tool transcribes on this PC",
      ],
    });
    rec.caption = "The text lands where you type";
    await sleep(200);
    await page.evaluate(() => {
      const s = document.getElementById("mailText");
      s.innerHTML =
        '<span class="new" id="fresh">Can we move the design review to Thursday afternoon? The new onboarding screens need one more pass before we show them to the team.</span>';
      setTimeout(
        () =>
          (document.getElementById("fresh").style.background = "transparent"),
        900,
      );
    });
    await sleep(2600);
    await rec.stop();
  },

  /** 2. History: search, hit counts, sort by most hits, copy. */
  async history(browser, dir) {
    const page = await openApp(browser, baseHandlers());
    await nav(page, "History");
    await page.waitForTimeout(800);
    const rec = new Recorder(page, dir);
    rec.caption = "Every take is kept in History";
    rec.start();
    await sleep(1200);
    rec.caption = "Search: the whole text, every hit marked and counted";
    await typeInto(
      page,
      page.getByPlaceholder("Search history (text or regex)..."),
      "budget",
    );
    await sleep(1500);
    rec.caption = "Sort by Most hits";
    await clickAt(page, page.getByRole("button", { name: "Newest first" }));
    await sleep(500);
    await clickAt(page, page.getByText("Most hits", { exact: true }).last());
    await sleep(1600);
    rec.caption = "Copy it, or paste it again anywhere";
    await clickAt(
      page,
      page.getByTitle("Copy transcription to clipboard").first(),
    );
    await sleep(1600);
    await rec.stop();
  },
};

Object.assign(STORIES, {
  /** 3. Models: download a local one, or use an online one. */
  async models(browser, dir) {
    const page = await openApp(browser, baseHandlers(), MODEL_FNS);
    await nav(page, "Models");
    await page.waitForTimeout(800);
    const rec = new Recorder(page, dir);
    rec.caption = "Pick the model that transcribes your voice";
    rec.start();
    await sleep(1200);
    rec.caption = "Local models run on this PC: download one once";
    await scrollTo(
      page,
      page.locator('[data-setting-title="Whisper Large V3"]'),
    );
    await clickAt(
      page,
      page
        .locator('[data-setting-title="Whisper Large V3"]')
        .getByRole("button", { name: "Download" }),
    );
    await sleep(500);
    await scrollTo(page, page.getByRole("button", { name: "Cancel" }));
    await sleep(2400);
    rec.caption = "It is ready, and it becomes your dictation model";
    await scrollTo(
      page,
      page.locator('[data-setting-title="Whisper Large V3"]'),
    );
    await sleep(1500);
    rec.caption =
      "Or use an online model: OpenRouter, an OpenAI-compatible API or your STT server";
    await scrollTo(
      page,
      page.locator('[data-setting-title="OpenRouter Transcription"]'),
    );
    await clickAt(
      page,
      page
        .locator('[data-setting-title="OpenRouter Transcription"]')
        .getByRole("button", { name: "Use this model" }),
    );
    await sleep(2200);
    await rec.stop();
  },

  /** 4. Jumper: anchor a field once, dictate from anywhere, it lands there. */
  async jumper(browser, dir) {
    const { page, frame } = await desktop(browser);
    await page.evaluate(() => {
      const mail = document.getElementById("mail");
      mail.style.left = "40px";
      mail.style.width = "470px";
      document.getElementById("chat").style.display = "block";
      document.getElementById("chat").classList.add("front");
      mail.classList.remove("front");
      document.getElementById("chatCaret").style.display = "inline-block";
    });
    const rec = new Recorder(page, dir);
    rec.caption =
      "In the chat box, press Ctrl+Shift+F1: it becomes your anchor";
    rec.start();
    await sleep(900);
    await keys(page, ["Ctrl", "+", "Shift", "+", "F1"]);
    await sleep(500);
    await page.evaluate(() => {
      document.getElementById("chatInput").classList.add("anchored");
      document.getElementById("badge").classList.add("on");
    });
    await sleep(900);
    await keys(page, []);
    rec.caption = "Keep working in another window";
    await page.evaluate(() => {
      document.getElementById("chat").classList.remove("front");
      document.getElementById("chatInput").classList.remove("anchored");
      document.getElementById("chatCaret").style.display = "none";
      const mail = document.getElementById("mail");
      mail.classList.add("front");
      mail.style.zIndex = 2;
    });
    await sleep(1300);
    rec.caption = "Dictate with Transcribe & Submit (Ctrl+Shift+F9)";
    await keys(page, ["Ctrl", "+", "Shift", "+", "F9"]);
    await sleep(400);
    await frame.evaluate(() => window.__emit("show-overlay", "recording"));
    await sleep(300);
    await keys(page, []);
    await speak(frame, 2600);
    await keys(page, ["Ctrl", "+", "Shift", "+", "F9"]);
    await frame.evaluate(() => window.__emit("show-overlay", "transcribing"));
    for (let p = 0; p <= 100; p += 12) {
      await frame.evaluate(
        (p) => window.__emit("transcription-progress", p),
        p,
      );
      await sleep(100);
    }
    await keys(page, []);
    await frame.evaluate(() => window.__emit("hide-overlay"));
    rec.caption = "Handy Tool jumps to the anchor, pastes and sends";
    await page.evaluate(() => {
      const mail = document.getElementById("mail");
      mail.classList.remove("front");
      mail.style.zIndex = 0;
      const chat = document.getElementById("chat");
      chat.style.zIndex = 3;
      chat.classList.add("front");
      document.getElementById("chatInput").classList.add("anchored");
      document.getElementById("chatText").textContent =
        "The release note is ready, I will post it before five.";
    });
    await sleep(900);
    await page.evaluate(() => {
      const m = document.createElement("div");
      m.className = "msg me";
      m.textContent = document.getElementById("chatText").textContent;
      document.getElementById("msgs").appendChild(m);
      document.getElementById("chatText").textContent = "";
    });
    await sleep(2200);
    await rec.stop();
  },

  /** 5. Model testing: one prompt, several AI models, side by side. */
  async compare(browser, dir) {
    const page = await openApp(
      browser,
      baseHandlers({ settings: { llm_providers: PROVIDERS } }),
      TEST_FNS,
    );
    await nav(page, "More Tools");
    await page.waitForTimeout(500);
    await page.getByText("Model Testing", { exact: true }).first().click();
    await page.waitForTimeout(800);
    const rec = new Recorder(page, dir);
    rec.caption = "Compare AI models on your own prompt";
    rec.start();
    await sleep(1000);
    await typeInto(
      page,
      page.getByPlaceholder("The prompt sent to every selected model…"),
      "Make this a friendly email: move the design review to Thursday",
    );
    rec.caption = "Tick the models to run: online or on this PC";
    await clickAt(page, page.getByText("Select all to run", { exact: true }));
    await sleep(800);
    rec.caption = "Run: answers, tokens, cost and time side by side";
    await clickAt(page, page.getByRole("button", { name: "Run test" }));
    await sleep(3000);
    await scrollTo(page, page.getByText("Summary", { exact: true }));
    await sleep(1800);
    await scrollTo(page, page.getByText("Answers", { exact: true }));
    await sleep(2200);
    await rec.stop();
  },
});

const browser = await chromium.launch({ channel: "msedge" });
for (const [name, story] of Object.entries(STORIES)) {
  if (ONLY.length && !ONLY.includes(name)) continue;
  console.log("story", name);
  await story(browser, `${OUT}/${name}`);
}
await browser.close();
