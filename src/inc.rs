use quinn::StreamId;
use crate::cli::parse::Arg;
use serde_derive::{Deserialize, Serialize};

pub enum RequestMessage {
	RequestJob,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamedJob {
	pub args: Vec<Arg>,
	pub output: StreamId,
	pub inputs: Vec<StreamId>,
}
