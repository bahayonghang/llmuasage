#[tokio::main]
async fn main() -> std::process::ExitCode {
    llmusage::sync::enable_live_pricing_refresh();
    llmusage::commands::entrypoint::process().await
}
