// SPDX-License-Identifier: Apache-2.0

//! Manage command line arguments

use crate::diagnostic::DiagnosticCommand;
use crate::registry::RegistryCommand;
use crate::serve::ServeCommand;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Command line arguments.
#[derive(Parser)]
#[command(
    author,
    version,
    about,
    long_about = None,
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    /// Turn debugging information on. Use twice (--debug --debug) for trace-level logs.
    #[arg(long, action = clap::ArgAction::Count, global = true)]
    pub debug: u8,

    /// Turn the quiet mode on (i.e., minimal output)
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Enable the most recent validation rules for the semconv registry. It is recommended
    /// to enable this flag when checking a new registry.
    /// Note: `semantic_conventions` main branch should always enable this flag.
    #[arg(long, global = true)]
    pub future: bool,

    /// Allow git credential helpers when cloning registries from private repositories.
    /// By default, git operations are isolated and cannot access global git config
    /// or credential helpers. Enable this flag to authenticate with private registries
    /// using your system's configured git credential helpers (e.g., osxkeychain,
    /// git-credential-manager).
    #[arg(long, global = true)]
    pub allow_git_credentials: bool,

    /// Path to a `.weaver.toml` project config file. When set, skips the
    /// upward-walk discovery from the current working directory.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// List of supported commands
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Supported commands.
#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Commands {
    /// Manage Semantic Convention Registry
    Registry(RegistryCommand),
    /// Plan, construct, and receipt cloud integrations from ggen Marketplace packs.
    ///
    /// This command is hidden while the BRCE runner protocol is experimental; it remains
    /// directly invokable as `weaver cloud ...` and is documented in `docs/cloud-marketplace.md`.
    #[command(hide = true)]
    Cloud(CloudArgs),
    /// Manage Diagnostic Messages
    Diagnostic(DiagnosticCommand),
    /// Generate shell completions
    Completion(CompletionCommand),
    /// Start the API server (Experimental)
    Serve(ServeCommand),
    /// Generate markdown help documentation
    #[command(hide = true)]
    MarkdownHelp,
}

/// ggen Marketplace-backed cloud lifecycle.
#[derive(Args, Debug)]
pub struct CloudArgs {
    /// SELECT, CONSTRUCT, or DO through the BRCE boundary.
    #[command(subcommand)]
    pub command: CloudCommand,
}

/// Cloud integration phases.
#[derive(Subcommand, Debug)]
pub enum CloudCommand {
    /// SELECT exact marketplace packs and write an immutable cloud plan.
    Plan(CloudPlanCommand),
    /// CONSTRUCT from an admitted plan by replaying it and running `ggen sync run`.
    Construct(CloudConstructCommand),
    /// DO through an explicit BRCE runner and require a bound execution receipt.
    Actuate(CloudActuateCommand),
}

/// Arguments for `weaver cloud plan`.
#[derive(Args, Debug)]
pub struct CloudPlanCommand {
    /// Local checkout of the ggen Marketplace.
    #[arg(long)]
    pub marketplace: PathBuf,

    /// Provider or cloud identity. This is data, not a hard-coded provider enum.
    #[arg(long)]
    pub provider: String,

    /// Exact marketplace pack to admit. Repeat for every required capability pack.
    #[arg(long = "pack", required = true)]
    pub packs: Vec<String>,

    /// New JSON plan path. Existing plans are never overwritten.
    #[arg(long, default_value = "weaver-cloud-plan.json")]
    pub output: PathBuf,

    /// Python 3.11+ interpreter used to execute the marketplace's canonical validator.
    #[arg(long, default_value = "python3")]
    pub python: PathBuf,
}

/// Arguments for `weaver cloud construct`.
#[derive(Args, Debug)]
pub struct CloudConstructCommand {
    /// JSON plan produced by `weaver cloud plan`.
    #[arg(long)]
    pub plan: PathBuf,

    /// Workspace that will receive the consumer `ggen.toml` and generated artifacts.
    #[arg(long)]
    pub workspace: PathBuf,

    /// ggen executable. Weaver invokes only `ggen sync run` in this phase.
    #[arg(long, default_value = "ggen")]
    pub ggen: PathBuf,

    /// Python 3.11+ interpreter used to replay marketplace admission.
    #[arg(long, default_value = "python3")]
    pub python: PathBuf,

    /// New construction receipt path. Defaults inside the workspace.
    #[arg(long)]
    pub receipt: Option<PathBuf>,
}

/// Arguments for `weaver cloud actuate`.
#[derive(Args, Debug)]
pub struct CloudActuateCommand {
    /// ALIVE construction receipt produced by `weaver cloud construct`.
    #[arg(long)]
    pub construction: PathBuf,

    /// Executable implementing the `weaver.brce.cloud-intent.v1` protocol.
    /// Weaver never invokes a provider CLI or IaC engine directly.
    #[arg(long)]
    pub brce_runner: PathBuf,

    /// Argument passed directly to the BRCE runner. Repeat as needed; no shell is used.
    #[arg(long = "brce-arg")]
    pub brce_args: Vec<String>,

    /// Operation delegated to BRCE, such as `apply`, `destroy`, or `verify`.
    #[arg(long, default_value = "apply")]
    pub operation: String,

    /// New BRCE receipt path. Existing receipts are never overwritten.
    #[arg(long)]
    pub receipt: PathBuf,

    /// New intent path. Defaults beside the receipt as `<stem>.intent.json`.
    #[arg(long)]
    pub intent: Option<PathBuf>,
}

#[derive(Args)]
pub struct CompletionCommand {
    /// The shell to generate the completions for
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,

    /// (Optional) The file to write the completions to. Defaults to STDOUT.
    #[arg(long, hide = true)]
    pub completion_file: Option<PathBuf>,
}