use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tabled::{Table, builder::Builder};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Counter(HashMap<String, u32>);

impl Counter {
    fn new() -> Self {
        Self(HashMap::new())
    }

    fn add(&mut self, name: String) {
        let count = self.0.entry(name).or_default();
        *count += 1;
    }

    fn remove(&mut self, name: &str) {
        if let Some(v) = self.0.get_mut(name) {
            *v -= 1;
            if *v == 0 {
                self.0.remove(name);
            }
        }
    }

    fn iter(&self) -> impl Iterator<Item = (&String, &u32)> {
        self.0.iter()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ClientTally(HashMap<String, u64>);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Status {
    start_time: DateTime<Utc>,
    tally: HashMap<String, ClientTally>,
    clients: Counter,
}

impl Status {
    pub(super) fn new() -> Self {
        Self {
            start_time: Utc::now(),
            tally: HashMap::new(),
            clients: Counter::new(),
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

    pub(super) fn start_time(&self) -> &DateTime<Utc> {
        &self.start_time
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

    pub(super) fn client_connected(&mut self, name: String) {
        self.clients.add(name);
    }

    pub(super) fn client_disconnected(&mut self, name: &str) {
        self.clients.remove(name);
    }

    pub(super) fn clients(&self) -> impl Iterator<Item = (&String, &u32)> {
        self.clients.iter()
    }
}
