use interprocess::local_socket::tokio::LocalSocketStream;
use anyhow::Result;
use futures_lite::io::{AsyncWriteExt, AsyncReadExt};

pub async fn handle_cli(mut conn: LocalSocketStream) -> Result<()> {
    let mut buf = [0; 1024];
    let n = conn.read(&mut buf).await?;
    println!("Received: {}", n);
    conn.write_all(b"Hello from server").await?;
    Ok(())
}
