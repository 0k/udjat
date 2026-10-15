use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    version,
    about = "List harness sessions over ACP stdio",
    after_help = "Examples:\n  udjat-harness list-sessions --cwd /work/project\n  udjat-harness list-sessions --json --command 'opencode acp'\n\nUDJAT_HARNESS_COMMAND overrides the default harness command (opencode acp)."
)]
pub struct Args {
    /// Force color output (default: detect terminal, respects NO_COLOR)
    #[arg(long, conflicts_with = "no_color")]
    pub color: bool,

    /// Disable color output
    #[arg(long)]
    pub no_color: bool,

    #[command(flatten)]
    pub logging: kal_log::LogArgs,

    #[command(subcommand)]
    pub action: Actions,
}

#[derive(Subcommand)]
pub enum Actions {
    /// List all sessions for a working directory
    #[command(
        after_help = "Examples:\n  udjat-harness list-sessions\n  udjat-harness list-sessions --cwd /work/project --json\n\nHuman output: <sessionId>  <local YYYY-MM-DD HH:MM>  <title>; missing values: -\nNDJSON keys: updated (UTC epoch seconds), id, cwd, title. Unavailable optional fields are omitted.\nThe harness command is passed verbatim; cwd is sent only in the ACP request."
    )]
    ListSessions {
        /// Working directory to filter (default: current directory; resolved absolutely)
        #[arg(long)]
        cwd: Option<PathBuf>,

        /// Harness command line; UDJAT_HARNESS_COMMAND overrides the default
        #[arg(long, env = "UDJAT_HARNESS_COMMAND", default_value = "opencode acp")]
        command: String,

        /// Print one NDJSON object per session instead of human-readable lines
        #[arg(long)]
        json: bool,
    },
}
