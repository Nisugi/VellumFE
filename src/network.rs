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
                let mut buf = Vec::new();
                match reader.read_until(b'\n', &mut buf).await {
                    Ok(0) => {
                        info!("Connection closed by server");
                        let _ = server_tx_clone.send(ServerMessage::Disconnected);
                        break;
                    }
                    Ok(n) => {
                        // Try to convert to UTF-8, filter out invalid bytes if it fails
                        match String::from_utf8(buf) {
                            Ok(line) => {
                                // Strip only the trailing newline, preserve blank lines
                                let line = line.trim_end_matches(&['\r', '\n']);
                                let _ = server_tx_clone.send(ServerMessage::Text(line.to_string()));
                            }
                            Err(e) => {
                                // Recover the buffer from the error
                                let buf = e.into_bytes();

                                // Collect invalid byte positions and values for logging
                                let mut invalid_bytes = Vec::new();
                                for (i, &byte) in buf.iter().enumerate() {
                                    // Check if this byte would cause UTF-8 validation to fail
                                    // Invalid bytes are typically 0x80-0x9F or 0xA0-0xFF when not part of valid UTF-8
                                    if byte >= 0x80 && byte < 0xC0 {
                                        // This is either a continuation byte or invalid single byte
                                        // Check if it's part of a valid multi-byte sequence
                                        let mut is_valid_continuation = false;
                                        if i > 0 {
                                            // Check if previous byte starts a multi-byte sequence
                                            let prev = buf[i-1];
                                            if prev >= 0xC0 && prev < 0xF8 {
                                                is_valid_continuation = true;
                                            }
                                        }
                                        if !is_valid_continuation {
                                            invalid_bytes.push((i, byte));
                                        }
                                    }
                                }

                                debug!("Filtered {} invalid UTF-8 bytes from {} byte message", invalid_bytes.len(), n);
                                if !invalid_bytes.is_empty() {
                                    debug!("Invalid bytes: {}", invalid_bytes.iter().map(|(i, b)| format!("0x{:02x}@{}", b, i)).collect::<Vec<_>>().join(", "));
                                }

                                // Filter out invalid bytes - keep only valid UTF-8
                                let cleaned: Vec<u8> = buf.iter()
                                    .enumerate()
                                    .filter(|(i, &b)| {
                                        !invalid_bytes.iter().any(|(pos, _)| pos == i)
                                    })
                                    .map(|(_, &b)| b)
                                    .collect();

                                // Convert cleaned bytes to string
                                match String::from_utf8(cleaned) {
                                    Ok(line) => {
                                        let line = line.trim_end_matches(&['\r', '\n']);
                                        let _ = server_tx_clone.send(ServerMessage::Text(line.to_string()));
                                    }
                                    Err(_) => {
                                        // If still invalid after filtering, use lossy as fallback
                                        debug!("Cleaned bytes still invalid, using lossy conversion");
                                        let lossy = String::from_utf8_lossy(&buf);
                                        let line = lossy.trim_end_matches(&['\r', '\n']);
                                        let _ = server_tx_clone.send(ServerMessage::Text(line.to_string()));
                                    }
                                }
                            }
                        }
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
