use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

#[derive(Debug, Clone)]
pub enum ServerMessage {
    Text(String),
    Connected,
    Disconnected,
}

pub struct LichConnection;

impl LichConnection {
    pub async fn start(
        host: &str,
        port: u16,
        server_tx: mpsc::UnboundedSender<ServerMessage>,
        mut command_rx: mpsc::UnboundedReceiver<String>,
    ) -> Result<()> {
        info!("Connecting to Lich at {}:{}...", host, port);

        let stream = TcpStream::connect(format!("{}:{}", host, port))
            .await
            .context("Failed to connect to Lich")?;

        info!("Connected successfully");

        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);

        // Send frontend PID
        let pid = std::process::id();
        let msg = format!("SET_FRONTEND_PID:{}\n", pid);
        writer.write_all(msg.as_bytes()).await?;
        writer.flush().await?;
        debug!("Sent frontend PID: {}", pid);

        let _ = server_tx.send(ServerMessage::Connected);

        // Spawn reader task
        let server_tx_clone = server_tx.clone();
        let read_handle = tokio::spawn(async move {
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        info!("Connection closed by server");
                        let _ = server_tx_clone.send(ServerMessage::Disconnected);
                        break;
                    }
                    Ok(_) => {
                        // Strip only the trailing newline, preserve blank lines
                        let line = line.trim_end_matches(&['\r', '\n']);
                        let _ = server_tx_clone.send(ServerMessage::Text(line.to_string()));
                    }
                    Err(e) => {
                        error!("Error reading from server: {}", e);
                        let _ = server_tx_clone.send(ServerMessage::Disconnected);
                        break;
                    }
                }
            }
        });

        // Writer task (runs in this function)
        let _write_result = async {
            while let Some(cmd) = command_rx.recv().await {
                debug!("Sending command: {}", cmd);
                if let Err(e) = writer.write_all(cmd.as_bytes()).await {
                    error!("Failed to write command: {}", e);
                    break;
                }
                if let Err(e) = writer.write_all(b"\n").await {
                    error!("Failed to write newline: {}", e);
                    break;
                }
                if let Err(e) = writer.flush().await {
                    error!("Failed to flush: {}", e);
                    break;
                }
            }
        }
        .await;

        // Wait for reader to finish
        let _ = read_handle.await;

        Ok(())
    }
}
