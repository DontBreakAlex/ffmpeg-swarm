use std::io::{Write, Read};

use crate::ipc::{CliToService, ServiceToCli};
use anyhow::Result;
use interprocess::local_socket::{LocalSocketStream, NameTypeSupport};


fn connect_socket() -> Result<LocalSocketStream> {
    let name = {
        use NameTypeSupport::*;
        match NameTypeSupport::ALWAYS_AVAILABLE {
            OnlyPaths => "/tmp/ffmpeg-swarm.sock",
            OnlyNamespaced | Both => "@ffmpeg-swarm.sock",
        }
    };
    let conn = LocalSocketStream::connect(name)?;
    Ok(conn)
}


pub fn send_command(cmd: CliToService) -> Result<ServiceToCli> {
    let mut conn = connect_socket()?;

    let vec = postcard::to_allocvec(&cmd)?;
    let msg_len = (vec.len() as u32).to_le_bytes();
    conn.write(&msg_len)?;
    conn.write_all(vec.as_slice())?;
    let mut buf = [0u8; 4];
    conn.read_exact(&mut buf)?;
    let len = u32::from_le_bytes(buf);
    let mut buf = vec![0u8; len as usize];
    conn.read_exact(&mut buf)?;
    let res = postcard::from_bytes(&buf)?;
    Ok(res)
}
