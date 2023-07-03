use std::{env::current_dir, fs, path::PathBuf};

use anyhow::{anyhow, Result};
use futures_lite::io::{AsyncReadExt, AsyncWriteExt};
use interprocess::local_socket::tokio::LocalSocketStream;

pub async fn handle_cli(mut conn: LocalSocketStream) -> Result<()> {
    let mut buf = [0; 1024];
    let n = conn.read(&mut buf).await?;
    println!("Received: {}", n);
    conn.write_all(b"Hello from server").await?;
    Ok(())
}

/// Represents a ffmpeg command that can be executed on a file or a directory
/// It contains a list of ffmpeg arguments and is the logical parent of multiple jobs
/// Tasks are created with the submit command
#[derive(Debug)]
pub struct Task {
    pub args: Vec<Arg>,
}

/// A job is always part of a task
/// It is the smallest unit of work that can be given to a worker
/// It describes the files on which a task shoyuld be executed
#[derive(Debug)]
pub struct Job {
    pub inputs: Vec<PathBuf>,
    pub output: PathBuf,
}

pub fn submit(args: Vec<String>) -> Result<()> {
    let FfmpegArgs {
        input,
        output,
        args,
    } = parse_ffmpeg_args(args)?;
    let job = Task { args };
    let files = validate_files(input, output)?;
    println!("Submitting task: {:?} {:?}", job, files);

    Ok(())
}

#[derive(Debug)]
pub enum Arg {
    Input(u32),
    Output,
    Other(String),
}

#[derive(Debug)]
pub struct FfmpegArgs {
    pub input: Vec<String>,
    pub output: String,
    pub args: Vec<Arg>,
}

// Parses a ffmpeg command like ffmpeg -f libx264 -i  input.avi -acodec libmp3lame -ab 128k -ar 44100 -vcodec mpeg2video -vf scale=160:128 -b 176k -r 15 -strict -1 -ab output.mpg
pub fn parse_ffmpeg_args(args: Vec<String>) -> Result<FfmpegArgs> {
    let mut iter = args.into_iter().peekable();
    let ffmpeg = iter.next().ok_or(anyhow!("No command specified"))?;
    if ffmpeg != "ffmpeg" {
        return Err(anyhow!("Unexpected command, got {}", ffmpeg));
    }

    let mut file_index = 0;
    let mut input: Vec<String> = Vec::new();
    let mut output = String::new();
    let mut args: Vec<Arg> = Vec::new();

    while let Some(arg) = iter.next() {
        if arg == "-i" {
            if let Some(i) = iter.next() {
                input.push(i);
                args.push(Arg::Input(file_index));
                file_index += 1;
            } else {
                return Err(anyhow!("No input file specified after -i"));
            }
        } else {
            if iter.peek().is_none() {
                output = arg;
                args.push(Arg::Output);
            } else {
                args.push(Arg::Other(arg));
            }
        }
    }

    Ok(FfmpegArgs {
        input,
        output,
        args,
    })
}

pub fn validate_files(mut input: Vec<String>, output: String) -> Result<Vec<Job>> {
    match input.len() {
        0 => Err(anyhow!("No input files specified")),
        1 => Ok(validate_single_input(input.pop().unwrap(), output)?),
        _ => Ok(vec![validate_multiple_inputs(input, output)?]),
    }
}

fn validate_single_input(input: String, output: String) -> Result<Vec<Job>> {
    let attr = fs::metadata(&input)?;
    let is_dir_job = attr.is_dir();

    let attr = fs::metadata(&output);
    match attr {
        Ok(attr) => {
            if !(is_dir_job) && attr.is_dir() {
                return Err(anyhow!("Output file already exists"));
            }
        }
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => {}
            _ => return Err(e.into()),
        },
    }

    // let output_path = fs::canonicalize(output)?; // Cannot canonicalize a non-existing path
    let output_path = current_dir()?.join(output);
    let mut jobs = Vec::new();

    if is_dir_job {
        create_jobs_for_dir(input.into(), output_path, &mut jobs)?;
    } else {
        jobs.push(Job {
            inputs: vec![fs::canonicalize(input)?],
            output: output_path,
        });
    }

    Ok(jobs)
}

fn create_jobs_for_dir(input: PathBuf, output: PathBuf, jobs: &mut Vec<Job>) -> Result<()> {
    let mut entries = fs::read_dir(input)?;

    while let Some(entry) = entries.next() {
        let entry = entry?;
        let path = entry.path();
        let attr = fs::metadata(&path)?;
        if attr.is_dir() {
            let new_output = output.clone().join(path.file_name().unwrap());
            create_jobs_for_dir(path, new_output, jobs)?;
        } else {
            let mut output = output.clone();
            output.push(path.file_name().unwrap());
            jobs.push(Job {
                inputs: vec![fs::canonicalize(path)?],
                output,
            });
        }
    }

    Ok(())
}

fn validate_multiple_inputs(input: Vec<String>, output: String) -> Result<Job> {
    for i in input.iter() {
        let attr = fs::metadata(i)?;
        if attr.is_dir() {
            return Err(anyhow!(
                "Directories are only supported when they are the only input"
            ));
        }
    }

    let attr = fs::metadata(&output);
    match attr {
        Ok(_) => {
            return Err(anyhow!("Output file already exists"));
        }
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => {}
            _ => return Err(e.into()),
        },
    }

    Ok(Job {
        inputs: input
            .into_iter()
            .map(|i| fs::canonicalize(i))
            .collect::<Result<Vec<_>, _>>()?,
        output: fs::canonicalize(output)?,
    })
}
