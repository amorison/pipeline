use std::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    pipeline::client::main().await
}
