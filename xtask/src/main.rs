use clap::{Parser, Subcommand};

mod tasks;

#[derive(Parser)]
#[command(about = "Repository maintenance tasks")]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Refresh apps/integrations/linear/graphql/schema.graphql.
    UpdateLinearSchema,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Task::UpdateLinearSchema => tasks::update_linear_schema(),
    }
}
