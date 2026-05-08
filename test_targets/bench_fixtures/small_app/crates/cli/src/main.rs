use clap::{Parser, Subcommand, ValueEnum};
use small_app_api::{WorkspaceFilter, seed_state};
use small_app_domain::{CreateWorkspace, Plan, Workspace};
use tracing::info;

#[derive(Debug, Parser)]
#[command(name = "small-app")]
struct Cli {
    #[arg(long, env = "SMALL_APP_LOG", default_value = "info")]
    log: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve {
        #[arg(long, default_value = "127.0.0.1:3000")]
        bind: String,
    },
    Create {
        slug: String,
        name: String,
        owner_email: String,
        #[arg(long, value_enum, default_value_t = CliPlan::Team)]
        plan: CliPlan,
    },
    List {
        #[arg(long, value_enum)]
        plan: Option<CliPlan>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliPlan {
    Free,
    Team,
    Enterprise,
}

impl From<CliPlan> for Plan {
    fn from(plan: CliPlan) -> Self {
        match plan {
            CliPlan::Free => Self::Free,
            CliPlan::Team => Self::Team,
            CliPlan::Enterprise => Self::Enterprise,
        }
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(cli.log)
        .without_time()
        .init();

    match cli.command {
        Command::Serve { bind } => {
            let state = seed_state().await;
            let router = small_app_api::router(state);
            info!(%bind, routes = 3, "server configured");
            drop(router);
        }
        Command::Create {
            slug,
            name,
            owner_email,
            plan,
        } => {
            let workspace = Workspace::from_request(CreateWorkspace {
                slug,
                name,
                owner_email,
                plan: plan.into(),
            })
            .expect("CLI arguments should describe a valid workspace");
            println!(
                "{}",
                serde_json::to_string_pretty(&workspace.summary())
                    .expect("workspace summary should serialize")
            );
        }
        Command::List { plan } => {
            let state = seed_state().await;
            let summaries = state
                .summaries(WorkspaceFilter {
                    plan: plan.map(Plan::from),
                    min_utilization: None,
                })
                .await;
            println!(
                "{}",
                serde_json::to_string_pretty(&summaries)
                    .expect("workspace summaries should serialize")
            );
        }
    }
}
