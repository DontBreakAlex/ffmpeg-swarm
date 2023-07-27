use anyhow::{anyhow, Result};
use chrono::NaiveDateTime;
use rusqlite::{Connection, OptionalExtension};
use serde_derive::{Deserialize, Serialize};
use std::net::IpAddr;
use uuid::Uuid;

use crate::server::read_or_generate_uuid;

#[derive(Serialize, Deserialize, Debug)]
pub struct AdvertiseMessage {
    pub oldest_job: NaiveDateTime,
    pub peer_ips: Vec<IpAddr>,
    pub peer_id: Uuid,
}

pub fn do_advertise(conn: &mut Connection) -> Result<Option<AdvertiseMessage>> {
    let mut stmt = conn.prepare("SELECT MIN(created_at) FROM jobs WHERE started_at IS NULL")?;
    let created_at = stmt.query_row((), |r| r.get(0)).optional()?.flatten();
    let peer_id = read_or_generate_uuid()?;

    Ok(created_at.map(|created_at| AdvertiseMessage {
        oldest_job: created_at,
        peer_ips: Vec::new(),
        peer_id: *peer_id,
    }))
}

impl AdvertiseMessage {
	pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
		Ok(Self {
			oldest_job: row.get(2)?,
			peer_ips: postcard::from_bytes(&row.get::<_, Vec<u8>>(1)?).map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Blob, Box::new(e)))?,
			peer_id: row.get(0)?,
		})
	}
}