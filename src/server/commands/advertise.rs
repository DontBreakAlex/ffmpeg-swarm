use std::net::IpAddr;
use anyhow::{anyhow, Result};
use rusqlite::{Connection, OptionalExtension};
use serde_derive::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::read_or_generate_uuid;

#[derive(Serialize, Deserialize, Debug)]
pub struct AdvertiseMessage {
    pub created_at: u64,
    pub peer_ips: Vec<IpAddr>,
    pub peer_id: Uuid,
}

pub fn do_advertise(conn: &mut Connection) -> Result<Option<AdvertiseMessage>> {
    let mut stmt = conn.prepare("SELECT CAST(strftime('%s', MIN(created_at)) AS INTEGER) FROM jobs WHERE started_at IS NULL")?;
    let created_at = stmt.query_row((), |r| r.get(0)).optional()?;
    let peer_id = read_or_generate_uuid()?;

    Ok(created_at.map(|created_at| AdvertiseMessage { created_at, peer_ips: Vec::new(), peer_id: *peer_id }))
}