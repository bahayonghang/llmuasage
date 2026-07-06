use std::{env, net::SocketAddr, time::Duration};

use anyhow::Result;
use clap::Parser;
use llmusage::{
    query::{Dashboard, QueryFilter},
    testing::Fixture,
    web,
};

#[derive(Debug, Parser)]
#[command(about = "Serve a sanitized fixture dashboard for documentation screenshots")]
struct Args {
    /// Preferred local port. Use 0 to ask the OS for an available port.
    #[arg(long, default_value_t = 37421)]
    port: u16,

    /// Number of deterministic synthetic rows to seed.
    #[arg(long, default_value_t = 12)]
    rows: usize,

    /// Exit automatically after this many seconds. Omit to run until Ctrl+C.
    #[arg(long)]
    timeout_secs: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let fixture = Fixture::new()?;
    fixture.seed_dashboard(args.rows)?;
    let snapshot = Dashboard::open(fixture.store())?.snapshot(&QueryFilter::default())?;

    // Keep the fixture alive after cloning the Store so the tempdir-backed DB
    // remains available while the Axum task is serving the screenshot page.
    let store = fixture.store().clone();
    let addr: SocketAddr = web::serve(store, Some(args.port)).await?;
    println!("Documentation fixture dashboard: http://{addr}");
    println!("Runtime root: {}", fixture.paths().root_dir.display());
    println!(
        "Data: sanitized local docs fixture with {} requested rows and {} buckets",
        args.rows, snapshot.overview.bucket_count
    );

    if let Some(timeout_secs) = args.timeout_secs {
        tokio::time::sleep(Duration::from_secs(timeout_secs)).await;
    } else if env::var_os("CI").is_some() {
        println!("CI detected; fixture served successfully and will now exit.");
    } else {
        tokio::signal::ctrl_c().await?;
    }

    Ok(())
}
