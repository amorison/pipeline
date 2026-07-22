use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tabled::{Table, builder::Builder};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ClientTally(HashMap<String, u64>);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Status {
    tally: HashMap<String, ClientTally>,
}

impl Status {
    pub(super) fn new() -> Self {
        Self {
            tally: HashMap::new(),
        }
    }

    pub(super) fn received(&mut self, client: &str, processing: &str) {
        let count = self
            .tally
            .entry(client.to_owned())
            .or_default()
            .0
            .entry(processing.to_owned())
            .or_insert(0);
        *count += 1;
    }

    pub(super) fn tally_table(&self) -> Table {
        let mut processing_groups: Vec<_> = self
            .tally
            .values()
            .flat_map(|ct| ct.0.keys())
            .collect::<HashSet<_>>()
            .into_iter()
            .cloned()
            .collect();
        processing_groups.sort_unstable();
        let ngroups = processing_groups.len();

        let mut builder = Builder::with_capacity(self.tally.len() + 1, ngroups + 1);

        let mut title_rec = Vec::with_capacity(ngroups + 1);
        title_rec.push(String::new());
        title_rec.extend(processing_groups.clone());
        builder.push_record(title_rec);

        let mut clients: Vec<_> = self.tally.keys().cloned().collect();
        clients.sort_unstable();

        for cname in clients {
            let ctally = &self.tally[&cname];
            let mut rec = Vec::with_capacity(ngroups + 1);
            rec.push(cname);
            for group in &processing_groups {
                let nreceived = ctally.0.get(group).copied().unwrap_or_default();
                rec.push(format!("{nreceived}"));
            }
            builder.push_record(&rec);
        }
        builder.index().column(0).name(None).build()
    }
}
