use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};

use super::{AdvertiseMessage, LocalJob};

#[derive(Debug)]
pub enum RunnableJob {
    Remote(AdvertiseMessage),
    Local(LocalJob),
}

pub fn do_acquire_job(conn: &mut Connection) -> Result<Option<RunnableJob>> {
    let tx = conn.transaction()?;
    let mut get_job = tx.prepare_cached("SELECT jobs.id, task_id, inputs, output, args FROM jobs INNER JOIN tasks t on t.id = jobs.task_id WHERE jobs.started_at IS NULL ORDER BY jobs.created_at LIMIT 1;")?;
    let mut start_task = tx.prepare_cached(
        "UPDATE tasks SET started_at = coalesce(started_at, CURRENT_TIMESTAMP) WHERE id = ?;",
    )?;
    let mut start_job =
        tx.prepare_cached("UPDATE jobs SET started_at = CURRENT_TIMESTAMP WHERE id = ?;")?;
    let mut get_peer = tx.prepare_cached(
        "SELECT uuid, ips, oldest_job FROM known_peers ORDER BY oldest_job LIMIT 1;",
    )?;

    let local_job = get_job.query_row([], LocalJob::from_row).optional()?;
    // let local_job = None::<LocalJob>;

    if let Some(local_job) = local_job {
        start_task.execute([local_job.task_id])?;
        start_job.execute([local_job.job_id])?;
        drop((get_job, start_task, start_job, get_peer));

        tx.commit()?;
        Ok(Some(RunnableJob::Local(local_job)))
    } else {
        let peer = get_peer
            .query_row([], AdvertiseMessage::from_row)
            .optional()?;

        if let Some(peer) = peer {
            drop((get_job, start_task, start_job, get_peer));

            tx.commit()?;
            Ok(Some(RunnableJob::Remote(peer)))
        } else {
            Ok(None)
        }
    }
}
