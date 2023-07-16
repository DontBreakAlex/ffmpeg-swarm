use anyhow::{anyhow, Result};
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
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
    println!("Parsing ffmpeg args: {:?}", args);
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
                args.push(Arg::Other(arg));
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
                if arg.ends_with(":") {
                    if let Some(next) = iter.next() {
                        args.push(Arg::Other(format!("{}{}", arg, next)));
                        continue;
                    }
                }
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
