use anyhow::Result;
use directories::ProjectDirs;
use rusqlite::Connection;

pub fn init() -> Result<()> {
    let proj_dirs = ProjectDirs::from("none", "aseo", "ffmpeg-swarm").unwrap();
    let path = proj_dirs.data_dir().join("ffmpeg-swarm.db");
    let conn = Connection::open(path)?;
    conn.execute_batch(include_str!("./init.sql"))?;

    Ok(())
}

pub fn sqlite_connect() -> Result<Connection> {
    let proj_dirs = ProjectDirs::from("none", "aseo", "ffmpeg-swarm").unwrap();
    let path = proj_dirs.data_dir().join("ffmpeg-swarm.db");
    let conn = Connection::open(path)?;

    Ok(conn)
}
