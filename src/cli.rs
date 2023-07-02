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

pub fn submit(args: Vec<String>) -> Result<()> {
    let args = parse_ffmpeg_args(args)?;
    println!("Submitting job: {:?}", args);

    Ok(())
}

#[derive(Debug)]
pub struct FfmpegArgs {
    pub input: Vec<String>,
    pub output: String,
    pub args: Vec<String>,
}

// Parses a ffmpeg command like ffmpeg -f libx264 -i  input.avi -acodec libmp3lame -ab 128k -ar 44100 -vcodec mpeg2video -vf scale=160:128 -b 176k -r 15 -strict -1 -ab output.mpg
pub fn parse_ffmpeg_args(args: Vec<String>) -> Result<FfmpegArgs> {
    let mut iter = args.into_iter().peekable();
    let ffmpeg = iter.next().ok_or(anyhow!("No command specified"))?;
    if ffmpeg != "ffmpeg" {
        return Err(anyhow!("Unexpected command, got {}", ffmpeg));
    }
    
    let mut input: Vec<String> = Vec::new();
    let mut output = String::new();
    let mut args: Vec<String> = Vec::new();

    while let Some(arg) = iter.next() {
        if arg == "-i" {
            if let Some(i) = iter.next() {
                input.push(i);
            } else {
                return Err(anyhow!("No input file specified after -i"));
            }
        } else {
            if iter.peek().is_none() {
                output = arg;
            } else {
                args.push(arg);
            }
        }
    }

    Ok(FfmpegArgs {
        input,
        output,
        args,
    })
}
