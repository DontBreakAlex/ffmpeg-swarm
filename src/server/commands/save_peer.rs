use anyhow::{Error, Result};
use rusqlite::{params, Connection, Transaction};
use tokio::sync::mpsc::Sender;

use crate::{
    db::SQLiteCommand,
    ipc::{Job, ServiceToCli, Task},
};

use super::AdvertiseMessage;

pub fn do_save_peer(conn: &mut Connection, peer_message: AdvertiseMessage) -> Result<()> {

    Ok(())
}
