pub mod parse;
mod validation;
mod ipc;

use anyhow::Result;
use crate::{cli::{parse::{FfmpegArgs, parse_ffmpeg_args}, validation::validate_files}, ipc::{Task, CliToService}};
use ipc::send_command;

pub fn submit(args: Vec<String>) -> Result<()> {
    let FfmpegArgs {
        input,
        output,
        args,
    } = parse_ffmpeg_args(args)?;
    let task = Task { args };
    let jobs = validate_files(input, output)?;
    let res = send_command(CliToService::SubmitJob { task, jobs })?;
    println!("{:?}", res);
    Ok(())
}
