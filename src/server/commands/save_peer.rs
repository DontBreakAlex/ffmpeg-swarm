use anyhow::Result;
use rusqlite::{params, Connection};

use super::AdvertiseMessage;

pub fn do_save_peer(conn: &mut Connection, msg: &AdvertiseMessage) -> Result<()> {
    let mut stmt =
        conn.prepare_cached("INSERT INTO known_peers (uuid, ips, oldest_job) VALUES (?, ?, ?);")?;
    stmt.execute(params![
        msg.peer_id,
        postcard::to_allocvec(&msg.peer_ips)?,
        msg.oldest_job
    ])?;

    Ok(())
}

pub fn do_remove_peer(conn: &mut Connection, peer_id: uuid::Uuid) -> Result<()> {
    let mut stmt = conn.prepare_cached("DELETE FROM known_peers WHERE uuid = ?;")?;
    stmt.execute(params![peer_id])?;

    Ok(())
}
