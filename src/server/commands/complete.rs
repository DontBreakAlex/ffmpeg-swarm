use anyhow::Result;
use rusqlite::{params, Connection};
use uuid::Uuid;

pub fn do_complete(
    conn: &mut Connection,
    job: u32,
    exit_code: i32,
    completed_by: Uuid,
) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE jobs SET exit_code = ?, finished_at = CURRENT_TIMESTAMP, completed_by = ? WHERE id = ?",
    )?;
    stmt.execute(params![exit_code, completed_by, job])?;

    // TODO Write finished_at to task if all jobs are finished

    Ok(())
}

pub fn do_reset_job(conn: &mut Connection, job_id: u32) -> Result<()> {
    let mut stmt = conn.prepare_cached("UPDATE jobs SET started_at = NULL WHERE id = ?")?;
    stmt.execute(params![job_id])?;

    Ok(())
}
