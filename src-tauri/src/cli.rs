use clap::{Parser, Subcommand};

#[derive(Parser, Debug, Clone, Default)]
#[command(name = "handy", about = "Handy - Speech to Text")]
pub struct CliArgs {
    /// Start with the main window hidden
    #[arg(long)]
    pub start_hidden: bool,

    /// Disable the system tray icon
    #[arg(long)]
    pub no_tray: bool,

    /// Toggle transcription on/off (sent to running instance)
    #[arg(long)]
    pub toggle_transcription: bool,

    /// Toggle transcription with post-processing on/off (sent to running instance)
    #[arg(long)]
    pub toggle_post_process: bool,

    /// Cancel the current operation (sent to running instance)
    #[arg(long)]
    pub cancel: bool,

    /// Enable debug mode with verbose logging
    #[arg(long)]
    pub debug: bool,

    /// CLI companion command. When present, Handy runs headlessly as a client to
    /// the running app's local MCP/CLI server instead of launching the GUI.
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// CLI companion subcommands. These talk to the running Handy app's localhost
/// server (auto-starting the app if needed); they do not launch the GUI.
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Run a prompt across providers (+ optional judge panel) and print/save a report
    ModelTest {
        /// Run providers (comma-separated ids or names)
        #[arg(long)]
        run: String,
        /// Judge providers (comma-separated ids or names)
        #[arg(long)]
        judge: Option<String>,
        /// Prompt text
        #[arg(long)]
        prompt: Option<String>,
        /// Read the prompt from a file
        #[arg(long = "prompt-file")]
        prompt_file: Option<String>,
        /// Judge (arbiter) prompt text
        #[arg(long = "judge-prompt")]
        judge_prompt: Option<String>,
        /// Read the judge prompt from a file
        #[arg(long = "judge-prompt-file")]
        judge_prompt_file: Option<String>,
        /// Use a saved preset (by name or id) for the prompts
        #[arg(long)]
        preset: Option<String>,
        #[arg(long = "model-temp", default_value_t = 0.3)]
        model_temp: f64,
        #[arg(long = "model-thinking", default_value = "auto")]
        model_thinking: String,
        #[arg(long = "judge-temp", default_value_t = 0.3)]
        judge_temp: f64,
        #[arg(long = "judge-thinking", default_value = "auto")]
        judge_thinking: String,
        /// Attach an image (path) for vision-capable runner models
        #[arg(long)]
        image: Option<String>,
        /// Write the Markdown report to this file (relative to the shell CWD)
        #[arg(long)]
        out: Option<String>,
        /// Print the full JSON result instead of the Markdown report
        #[arg(long)]
        json: bool,
    },
    /// Count tokens in text
    TokenCount {
        text: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long, default_value = "cl100k_base")]
        tokenizer: String,
    },
    /// Type text into the focused window
    Type {
        text: Option<String>,
        #[arg(long)]
        file: Option<String>,
    },
    /// List recent transcription history
    HistoryList {
        #[arg(long)]
        limit: Option<u64>,
    },
    /// Get one transcription history entry by id
    HistoryGet {
        #[arg(long)]
        id: i64,
    },
    /// List registered LLM providers (API keys redacted)
    ProvidersList,
    /// Update a provider by id (changing the model auto-fills cost)
    ProvidersSet {
        #[arg(long)]
        id: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long = "api-key")]
        api_key: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long = "base-url")]
        base_url: Option<String>,
        #[arg(long)]
        enabled: Option<bool>,
        #[arg(long)]
        sequential: Option<bool>,
        #[arg(long = "concurrency-group")]
        concurrency_group: Option<String>,
        #[arg(long = "persist-price")]
        persist_price: Option<bool>,
        #[arg(long = "cost-input")]
        cost_input: Option<f64>,
        #[arg(long = "cost-output")]
        cost_output: Option<f64>,
    },
    /// Query a provider's available models live from its API
    ProvidersModels {
        #[arg(long)]
        id: String,
    },
    /// Run the stdio MCP bridge (for Claude Code); proxies to the app's MCP server
    Mcp {
        #[arg(long)]
        stdio: bool,
    },
    /// Install the `handy` CLI onto the user PATH
    InstallCli,
}
