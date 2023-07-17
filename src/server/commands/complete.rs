use anyhow::Result;
use rusqlite::{params, Connection};

pub fn do_complete(conn: &mut Connection, job: u32, success: bool) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE jobs SET success = ?, finished_at = CURRENT_TIMESTAMP WHERE id = ?",
    )?;
    stmt.execute(params![success, job])?;

    // TODO Write finished_at to task if all jobs are finished

    Ok(())
}
