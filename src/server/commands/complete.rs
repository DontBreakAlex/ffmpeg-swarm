use anyhow::Result;
use rusqlite::{params, Connection};

pub fn do_complete(conn: &mut Connection, job: u32, exit_code: i32) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE jobs SET exit_code = ?, finished_at = CURRENT_TIMESTAMP WHERE id = ?",
    )?;
    stmt.execute(params![exit_code, job])?;

    // TODO Write finished_at to task if all jobs are finished

    Ok(())
}
