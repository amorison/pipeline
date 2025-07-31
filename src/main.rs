use std::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(
        env_logger::Env::default()
            .filter_or("PIPELINE_LOG", "info")
            .write_style_or("PIPELINE_LOG_STYLE", "auto"),
    );
    pipeline::cli::main().await
}
