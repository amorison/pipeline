use std::sync::Arc;

use tokio::{fs, io};

use crate::server::Config;

pub(crate) async fn main(config: Config) -> io::Result<()> {
    let config = Arc::new(config);
    let mut handles = Vec::with_capacity(256);

    for i in 0..256 {
        let conf = config.clone();
        let handle = tokio::spawn(async move {
            for j in 0..256 {
                let bucket = conf.incoming_path(format!("{i:02x}/{j:02x}"));
                fs::create_dir_all(bucket)
                    .await
                    .expect("cannot create {bucket}");
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await?;
    }

    Ok(())
}
