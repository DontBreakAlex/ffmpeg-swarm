use std::path::PathBuf;
use crate::cli::parse::Arg;

use serde_derive::{Serialize, Deserialize};

/// Represents a ffmpeg command that can be executed on a file or a directory
/// It contains a list of ffmpeg arguments and is the logical parent of multiple jobs
/// Tasks are created with the submit command
#[derive(Debug, Serialize, Deserialize)]
pub struct Task {
    pub args: Vec<Arg>,
}

/// A job is always part of a task
/// It is the smallest unit of work that can be given to a worker
/// It describes the files on which a task shoyuld be executed
#[derive(Debug, Serialize, Deserialize)]
pub struct Job {
    pub inputs: Vec<PathBuf>,
    pub output: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CliToService {
    SubmitJob {
        task: Task,
        jobs: Vec<Job>
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServiceToCli {
    Ok,
}