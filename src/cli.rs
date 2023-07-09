mod ipc;
pub mod parse;
mod validation;

use crate::{
    cli::{
        parse::{parse_ffmpeg_args, FfmpegArgs},
        validation::validate_files,
    },
    ipc::{CliToService, Task},
};
use anyhow::Result;
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
    println!("{:?} ---", res);
    Ok(())
}
