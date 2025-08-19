use tabled::{Table, settings::Style};
use tokio::io;

use crate::server::database::Database;

pub(crate) async fn main() -> io::Result<()> {
    let db = Database::read_only().await.unwrap();
    let content = db.content().await.unwrap();
    let mut table = Table::new(&content);
    table.with(
        Style::markdown()
            .remove_vertical()
            .remove_left()
            .remove_right(),
    );
    println!("{}", table);
    Ok(())
}
