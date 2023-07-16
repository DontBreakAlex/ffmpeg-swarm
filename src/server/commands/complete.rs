use rusqlite::{Connection, params};
use anyhow::Result;


pub fn do_complete(conn: &mut Connection, job: u32, success: bool) -> Result<()> {
    let mut stmt = conn.prepare_cached("UPDATE jobs SET success = ? WHERE id = ?")?;
    stmt.execute(params![success, job])?;

    Ok(())
}