use std::path::PathBuf;
use std::time::Duration;

use adapter_rmvm::RmvmAdapter;
use anyhow::{Result, bail};
use brain_store::{AttachmentGrant, BrainStore, CreateBrainRequest, MergeStrategy};
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::proxy::{PlannerConfig, PlannerMode, ProxyConfig, parse_addr, serve};

#[derive(Debug, Parser)]
#[command(name = "cortex", about = "Portable Brain + Proxy UX CLI")]
pub struct Cli {
    #[command(subcommand)]
    command: TopCommand,
}

#[derive(Debug, Subcommand)]
enum TopCommand {
    Brain {
        #[command(subcommand)]
        command: BrainCommand,
    },
    Proxy {
        #[command(subcommand)]
        command: ProxyCommand,
    },
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
}

#[derive(Debug, Subcommand)]
enum BrainCommand {
    Create(CreateCmd),
    #[command(alias = "open")]
    Use(UseCmd),
    List(ListCmd),
    Export(ExportCmd),
    Import(ImportCmd),
    Branch(BranchCmd),
    Merge(MergeCmd),
    Forget(ForgetCmd),
    Attach(AttachCmd),
    Detach(DetachCmd),
    Audit(AuditCmd),
}

#[derive(Debug, Subcommand)]
enum ProxyCommand {
    Serve(ServeCmd),
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    MapKey(MapKeyCmd),
}

#[derive(Debug, Args)]
struct CreateCmd {
    name: String,
    #[arg(long)]
    path: Option<PathBuf>,
    #[arg(long, default_value = "local")]
    tenant: String,
    #[arg(long)]
    passphrase_env: Option<String>,
}

#[derive(Debug, Args)]
struct UseCmd {
    brain: String,
}

#[derive(Debug, Args)]
struct ListCmd {
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct ExportCmd {
    brain: String,
    #[arg(long)]
    out: PathBuf,
    #[arg(long)]
    signing_key: Option<String>,
}

#[derive(Debug, Args)]
struct ImportCmd {
    #[arg(long = "in")]
    input: PathBuf,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    verify_only: bool,
}

#[derive(Debug, Args)]
struct BranchCmd {
    brain: String,
    #[arg(long = "new")]
    new_branch: String,
}

#[derive(Debug, ValueEnum, Clone)]
enum MergeStrategyArg {
    Ours,
    Theirs,
    Manual,
}

#[derive(Debug, Args)]
struct MergeCmd {
    #[arg(long)]
    source: String,
    #[arg(long)]
    target: String,
    #[arg(long, value_enum, default_value = "ours")]
    strategy: MergeStrategyArg,
    #[arg(long)]
    brain: Option<String>,
}

#[derive(Debug, Args)]
struct ForgetCmd {
    #[arg(long)]
    subject: String,
    #[arg(long = "predicate")]
    predicate: String,
    #[arg(long, default_value = "SCOPE_GLOBAL")]
    scope: String,
    #[arg(long, default_value = "suppress preference")]
    reason: String,
    #[arg(long)]
    brain: Option<String>,
}

#[derive(Debug, Args)]
struct AttachCmd {
    #[arg(long = "agent")]
    agent: String,
    #[arg(long = "model")]
    model: String,
    #[arg(long)]
    read: String,
    #[arg(long)]
    write: String,
    #[arg(long)]
    sinks: String,
    #[arg(long)]
    ttl: Option<String>,
    #[arg(long)]
    brain: Option<String>,
}

#[derive(Debug, Args)]
struct DetachCmd {
    #[arg(long = "agent")]
    agent: String,
    #[arg(long = "model")]
    model: Option<String>,
    #[arg(long)]
    brain: Option<String>,
}

#[derive(Debug, Args)]
struct AuditCmd {
    #[arg(long)]
    since: Option<String>,
    #[arg(long)]
    until: Option<String>,
    #[arg(long)]
    subject: Option<String>,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    brain: Option<String>,
}

#[derive(Debug, Args)]
struct ServeCmd {
    #[arg(long, default_value = "127.0.0.1:8080")]
    addr: String,
    #[arg(
        long,
        env = "CORTEX_ENDPOINT",
        default_value = "grpc://127.0.0.1:50051"
    )]
    endpoint: String,
    #[arg(long, env = "CORTEX_BRAIN")]
    brain: Option<String>,
    #[arg(long, env = "CORTEX_PLANNER_MODE", default_value = "fallback")]
    planner_mode: String,
    #[arg(
        long,
        env = "CORTEX_PLANNER_BASE_URL",
        default_value = "https://api.openai.com/v1"
    )]
    planner_base_url: String,
    #[arg(long, env = "CORTEX_PLANNER_MODEL", default_value = "gpt-4o-mini")]
    planner_model: String,
    #[arg(long, env = "CORTEX_PLANNER_API_KEY")]
    planner_api_key: Option<String>,
    #[arg(long, env = "CORTEX_PLANNER_TIMEOUT_SECS", default_value = "30")]
    planner_timeout_secs: u64,
}

#[derive(Debug, Args)]
struct MapKeyCmd {
    #[arg(long = "api-key")]
    api_key: String,
    #[arg(long)]
    tenant: String,
    #[arg(long)]
    brain: String,
    #[arg(long, default_value = "user:local")]
    subject: String,
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        TopCommand::Brain { command } => handle_brain(command).await,
        TopCommand::Proxy { command } => handle_proxy(command).await,
        TopCommand::Auth { command } => handle_auth(command).await,
    }
}

async fn handle_brain(cmd: BrainCommand) -> Result<()> {
    let store = BrainStore::new(None)?;
    match cmd {
        BrainCommand::Create(c) => {
            let store = if let Some(path) = c.path {
                BrainStore::new(Some(path))?
            } else {
                store
            };
            let created = store.create_brain(CreateBrainRequest {
                name: c.name,
                tenant_id: c.tenant,
                passphrase_env: c.passphrase_env,
            })?;
            println!("Created brain {} ({})", created.name, created.brain_id);
            println!("Set active with: cortex brain use {}", created.brain_id);
        }
        BrainCommand::Use(c) => {
            let s = store.set_active_brain(&c.brain)?;
            println!("Active brain set: {} ({})", s.name, s.brain_id);
        }
        BrainCommand::List(c) => {
            let list = store.list_brains()?;
            if c.json {
                println!("{}", serde_json::to_string_pretty(&list)?);
            } else {
                let active = store.active_brain_id()?;
                for b in list {
                    let marker = if active.as_ref() == Some(&b.brain_id) {
                        "*"
                    } else {
                        " "
                    };
                    println!(
                        "{} {} [{}] tenant={} branch={}",
                        marker, b.name, b.brain_id, b.tenant_id, b.active_branch
                    );
                }
            }
        }
        BrainCommand::Export(c) => {
            let _ = c.signing_key;
            store.export_brain(&c.brain, &c.out)?;
            println!("Exported brain {} to {}", c.brain, c.out.display());
        }
        BrainCommand::Import(c) => {
            let res = store.import_brain(&c.input, c.name, c.verify_only)?;
            if c.verify_only {
                println!("Import verification passed: {}", c.input.display());
            } else if let Some(summary) = res {
                println!("Imported brain {} ({})", summary.name, summary.brain_id);
            }
        }
        BrainCommand::Branch(c) => {
            store.branch(&c.brain, &c.new_branch)?;
            println!("Created branch {} in {}", c.new_branch, c.brain);
        }
        BrainCommand::Merge(c) => {
            let strategy = match c.strategy {
                MergeStrategyArg::Ours => MergeStrategy::Ours,
                MergeStrategyArg::Theirs => MergeStrategy::Theirs,
                MergeStrategyArg::Manual => MergeStrategy::Manual,
            };
            let brain = store.resolve_brain_or_active(c.brain.as_deref())?;
            let report = store.merge(&brain.brain_id, &c.source, &c.target, strategy)?;
            println!(
                "Merged source={} target={} merged={} conflicts={}",
                c.source,
                c.target,
                report.merged,
                report.conflicts.len()
            );
        }
        BrainCommand::Forget(c) => {
            let brain = store.resolve_brain_or_active(c.brain.as_deref())?;
            let suppressed = store.forget_suppress(
                &brain.brain_id,
                &c.subject,
                &c.predicate,
                &c.scope,
                &c.reason,
            )?;
            println!(
                "Suppressed {} objects for subject={} predicate={}",
                suppressed, c.subject, c.predicate
            );
        }
        BrainCommand::Attach(c) => {
            let brain = store.resolve_brain_or_active(c.brain.as_deref())?;
            store.attach(
                &brain.brain_id,
                AttachmentGrant {
                    agent_id: c.agent,
                    model_id: c.model,
                    read_classes: split_csv(&c.read),
                    write_classes: split_csv(&c.write),
                    sinks: split_csv(&c.sinks),
                    expires_at: c.ttl,
                },
            )?;
            println!("Attachment saved for brain {}", brain.brain_id);
        }
        BrainCommand::Detach(c) => {
            let brain = store.resolve_brain_or_active(c.brain.as_deref())?;
            let removed = store.detach(&brain.brain_id, &c.agent, c.model.as_deref())?;
            println!("Removed {} attachment(s)", removed);
        }
        BrainCommand::Audit(c) => {
            let brain = store.resolve_brain_or_active(c.brain.as_deref())?;
            let mut rows = store.audit_trace(&brain.brain_id)?;
            if let Some(subject) = c.subject {
                rows.retain(|r| r.details.to_string().contains(&subject));
            }
            if c.since.is_some() || c.until.is_some() {
                // v0: filters accepted for UX compatibility; strict timestamp filtering lands in next cut.
            }
            if c.json {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else {
                for row in rows {
                    println!("{} {} {} {}", row.ts, row.actor, row.action, row.details);
                }
            }
        }
    }
    Ok(())
}

async fn handle_proxy(cmd: ProxyCommand) -> Result<()> {
    match cmd {
        ProxyCommand::Serve(c) => {
            let _ = RmvmAdapter::new(c.endpoint.clone());
            let bind_addr = parse_addr(&c.addr)?;
            let planner_mode = PlannerMode::parse(&c.planner_mode)?;
            serve(ProxyConfig {
                bind_addr,
                endpoint: c.endpoint,
                default_brain: c.brain,
                brain_home: None,
                planner: PlannerConfig {
                    mode: planner_mode,
                    base_url: c.planner_base_url,
                    model: c.planner_model,
                    api_key: c
                        .planner_api_key
                        .or_else(|| std::env::var("OPENAI_API_KEY").ok()),
                    timeout: Duration::from_secs(c.planner_timeout_secs),
                },
            })
            .await
        }
    }
}

async fn handle_auth(cmd: AuthCommand) -> Result<()> {
    let store = BrainStore::new(None)?;
    match cmd {
        AuthCommand::MapKey(c) => {
            let brain = store.resolve_brain(&c.brain)?;
            if brain.tenant_id != c.tenant {
                bail!(
                    "tenant mismatch: brain tenant={} but --tenant={}",
                    brain.tenant_id,
                    c.tenant
                );
            }
            store.map_api_key(&c.api_key, &c.tenant, &brain.brain_id, &c.subject)?;
            println!("Mapped API key to brain {}", brain.brain_id);
        }
    }
    Ok(())
}

fn split_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
