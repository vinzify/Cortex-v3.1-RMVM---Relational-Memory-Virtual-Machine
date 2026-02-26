mod cli;
mod proxy;
mod types;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,cortex_app=debug".to_string()),
        )
        .with_target(false)
        .compact()
        .init();

    cli::run().await
}
