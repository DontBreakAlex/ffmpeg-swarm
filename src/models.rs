use std::path::PathBuf;

use diesel::prelude::*;
use serde_derive::{Serialize, Deserialize};

#[derive(Selectable, Insertable, Debug, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::jobs)]
/// A job is always part of a task
/// It is the smallest unit of work that can be given to a worker
/// It describes the files on which a task shoyuld be executed
pub struct Job {
    pub inputs: Vec<String>,
    pub output: String,
}