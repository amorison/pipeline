use std::collections::HashMap;

use serde::{Deserialize, Deserializer};

pub(crate) fn vec_at_least_one<'de, D, T>(de: D) -> Result<Vec<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    let vec = Vec::deserialize(de)?;
    if vec.is_empty() {
        return Err(serde::de::Error::custom("list should not be empty"));
    }
    Ok(vec)
}

pub(crate) fn map_at_least_one<'de, D, T>(de: D) -> Result<HashMap<String, T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    let map = HashMap::deserialize(de)?;
    if map.is_empty() {
        return Err(serde::de::Error::custom("map should not be empty"));
    }
    Ok(map)
}
