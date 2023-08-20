use crate::cli::parse::Arg;
use serde_derive::{Deserialize, Serialize};
use std::ffi::OsString;

#[derive(Debug, Serialize, Deserialize)]
pub enum RequestMessage {
    RequestJob,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamedJob {
    pub args: Vec<Arg>,
    pub input_count: usize,
    pub extension: OsString,
}
