pub mod app;

fn main() -> anyhow::Result<()> {
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let args = <app::Args as clap::Parser>::parse();
    app::run_app(&args)
}
