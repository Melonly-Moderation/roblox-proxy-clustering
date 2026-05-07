#[tokio::main]
async fn main() -> Result<(), roblox_proxy_clustering::error::AppError> {
    roblox_proxy_clustering::app::run().await
}
