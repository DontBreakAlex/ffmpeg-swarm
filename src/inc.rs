use crate::cli::parse::Arg;
use serde_derive::{Deserialize, Serialize};
use std::ffi::OsString;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub enum RequestMessage {
    RequestJob { requester_uuid: Uuid },
    Identify,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamedJob {
    pub args: Vec<Arg>,
    pub input_count: usize,
    pub extension: OsString,
    /// high bits are job_id, low bits are task_id
    pub stream_id: u64,
}
