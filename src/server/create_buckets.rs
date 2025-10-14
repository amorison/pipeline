use std::fs;

use tokio::io;

use crate::server::Config;

pub(crate) async fn main(config: Config) -> io::Result<()> {
    for i in 0..256 {
        for j in 0..256 {
            let bucket = config.incoming_path(format!("{i:02x}/{j:02x}"));
            fs::create_dir_all(bucket)?;
        }
    }
    Ok(())
}
