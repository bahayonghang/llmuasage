#[tokio::main]
async fn main() -> anyhow::Result<()> {
    llmusage::sync::enable_live_pricing_refresh();
    llmusage::run().await
}
