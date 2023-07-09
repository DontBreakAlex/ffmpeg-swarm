use crate::ipc::Job;
use anyhow::{anyhow, Result};
use std::{env::current_dir, fs, path::PathBuf};

pub fn validate_files(mut input: Vec<String>, output: String) -> Result<Vec<Job>> {
    match input.len() {
        0 => Err(anyhow!("No input files specified")),
        1 => Ok(validate_single_input(input.pop().unwrap(), output)?),
        _ => Ok(vec![validate_multiple_inputs(input, output)?]),
    }
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
