//! Rust port of the frontend report + judge-prompt assembly
//! (src/components/settings/model-testing/ModelTestingPage.tsx) so MCP/CLI
//! model-test runs produce the SAME judge behaviour and the SAME Markdown
//! artifact as the UI.

use crate::model_testing::{ChatOutcome, ModelTestRun};

fn fmt_tok(n: Option<u32>) -> String {
    match n {
        // Group thousands with commas to match the UI's `toLocaleString()`.
        Some(v) => {
            let digits = v.to_string();
            let bytes = digits.as_bytes();
            let len = bytes.len();
            let mut out = String::with_capacity(len + len / 3);
            for (i, b) in bytes.iter().enumerate() {
                if i > 0 && (len - i) % 3 == 0 {
                    out.push(',');
                }
                out.push(*b as char);
            }
            out
        }
        None => "—".to_string(),
    }
}

fn fmt_time(ms: u32) -> String {
    if ms >= 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

fn fmt_cost(o: &ChatOutcome) -> String {
    match o.cost_usd {
        None => "—".to_string(),
        Some(c) => format!("{}${:.4}", if o.cost_is_real { "" } else { "~" }, c),
    }
}

fn total_cost(run: &ModelTestRun) -> f64 {
    run.outcomes.iter().filter_map(|o| o.cost_usd).sum()
}

fn summary_table(rows: &[ChatOutcome]) -> String {
    let head = "| Model | Input tok | Output tok | Cost | Time |\n|---|---:|---:|---:|---:|";
    let body = rows
        .iter()
        .map(|o| {
            format!(
                "| {} ({}){} | {} | {} | {} | {} |",
                o.provider_name,
                o.model,
                if o.ok { "" } else { " — FAILED" },
                fmt_tok(o.input_tokens),
                fmt_tok(o.output_tokens),
                fmt_cost(o),
                fmt_time(o.elapsed_ms),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}\n{}", head, body)
}

fn answer_blocks(rows: &[ChatOutcome]) -> String {
    rows.iter()
        .map(|o| {
            let body = if o.ok {
                o.content.clone()
            } else {
                format!(
                    "**Error:** {}",
                    o.error.clone().unwrap_or_else(|| "unknown".into())
                )
            };
            format!("### {} ({})\n\n{}", o.provider_name, o.model, body)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Build the judge request so EVERY candidate answer is visible AND the arbiter
/// instructions live in the USER message (small/local models down-weight the
/// system prompt). Mirrors `buildJudgePrompt` in the frontend.
pub fn build_judge_prompt(
    arbiter: &str,
    input: &str,
    outcomes: &[ChatOutcome],
) -> (String, String) {
    let answered: Vec<&ChatOutcome> = outcomes.iter().filter(|o| o.ok).collect();
    let n = answered.len();
    let numbered = answered
        .iter()
        .enumerate()
        .map(|(i, o)| {
            format!(
                "<answer index=\"{}\" provider=\"{}\" model=\"{}\">\n{}\n</answer>",
                i + 1,
                o.provider_name,
                o.model,
                o.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let user = [
        "You are judging multiple candidate answers to the same prompt.".to_string(),
        format!(
            "There are {} answers, labelled <answer index=\"1\"> through <answer index=\"{}\">. Read and weigh ALL of them — do not judge only the first.",
            n, n
        ),
        String::new(),
        "# Evaluation task".to_string(),
        arbiter.trim().to_string(),
        String::new(),
        "# Original prompt given to the models".to_string(),
        format!("<original_prompt>\n{}\n</original_prompt>", input.trim()),
        String::new(),
        format!("# Candidate answers ({})", n),
        format!("<answers count=\"{}\">", n),
        numbered,
        "</answers>".to_string(),
    ]
    .join("\n");
    let system = format!(
        "You are an impartial evaluator comparing {} candidate answers. Consider every answer, then complete the user's evaluation task.",
        n
    );
    (system, user)
}

/// Inputs for building the Markdown report artifact.
pub struct ReportInput<'a> {
    pub timestamp: &'a str,
    pub main_prompt: &'a str,
    pub model_temperature: f64,
    pub model_thinking: &'a str,
    pub main_run: &'a ModelTestRun,
    pub judge_prompt: Option<&'a str>,
    pub judge_temperature: f64,
    pub judge_thinking: &'a str,
    pub judge_run: Option<&'a ModelTestRun>,
}

/// Build the Markdown artifact (input → summary → judge panel → answers).
pub fn build_report(inp: &ReportInput) -> String {
    let mut out: Vec<String> = Vec::new();
    out.push(format!("# Model Testing — {}", inp.timestamp));
    out.push(String::new());
    out.push("## Input".to_string());
    out.push(String::new());
    out.push("```".to_string());
    out.push(inp.main_prompt.trim().to_string());
    out.push("```".to_string());
    out.push(String::new());
    out.push("## Summary".to_string());
    out.push(String::new());
    out.push(summary_table(&inp.main_run.outcomes));
    out.push(String::new());
    out.push(format!(
        "**Round-trip (longest):** {} · **Total cost:** ${:.4}",
        fmt_time(inp.main_run.round_trip_ms),
        total_cost(inp.main_run)
    ));
    out.push(format!(
        "**Model params:** temperature {:.2} · thinking {}",
        inp.model_temperature, inp.model_thinking
    ));
    out.push(String::new());
    if let Some(judge_run) = inp.judge_run {
        out.push("## Judge Panel".to_string());
        out.push(String::new());
        out.push("> **Arbiter prompt:**".to_string());
        for l in inp.judge_prompt.unwrap_or("").trim().split('\n') {
            out.push(format!("> {}", l));
        }
        out.push(String::new());
        out.push(summary_table(&judge_run.outcomes));
        out.push(String::new());
        out.push(format!(
            "**Round-trip (longest):** {} · **Total cost:** ${:.4}",
            fmt_time(judge_run.round_trip_ms),
            total_cost(judge_run)
        ));
        out.push(format!(
            "**Judge params:** temperature {:.2} · thinking {}",
            inp.judge_temperature, inp.judge_thinking
        ));
        out.push(String::new());
        out.push(answer_blocks(&judge_run.outcomes));
        out.push(String::new());
    }
    out.push("## Answers".to_string());
    out.push(String::new());
    out.push(answer_blocks(&inp.main_run.outcomes));
    out.push(String::new());
    out.join("\n")
}
