//! Tokio-based client for the Lich proxy.
//!
//! Handles connecting to the chosen host/port, wiring async reader/writer loops,
//! and funneling everything through mpsc channels so the rest of the app stays
//! decoupled from direct socket management.

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{debug, error, info};

use chrono::Local;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc as std_mpsc, Arc};
use std::thread;
use std::time::Duration as StdDuration;

/// Messages emitted by the TCP reader task.
#[derive(Debug, Clone)]
pub enum ServerMessage {
    Text(String),
    Connected,
    Disconnected,
}

/// Stub type that exposes the async `start` helper.
pub struct LichConnection;

/// Runtime configuration for direct (non-Lich) connections.
pub struct DirectConnectConfig {
    pub account: String,
    pub password: String,
    pub character: String,
    pub game_code: String,
    pub data_dir: PathBuf,
}

struct LogWriterSettings {
    dir: PathBuf,
    buffer_lines: usize,
    flush_interval: StdDuration,
    max_lines_per_file: usize,
    timestamps: bool,
}

/// Raw XML logger for network input (pre-parse).
#[derive(Clone)]
pub struct RawLogger {
    tx: std_mpsc::SyncSender<String>,
    dropped: Arc<AtomicUsize>,
}

impl RawLogger {
    pub fn new(config: &crate::config::Config) -> Result<Option<Self>> {
        if !config.logging.enabled {
            return Ok(None);
        }

        let dir = config.logging.resolve_dir(config.character.as_deref())?;
        let buffer_lines = config.logging.buffer_lines.max(1);
        let flush_interval =
            StdDuration::from_millis(config.logging.flush_interval_ms.max(1));
        let max_lines_per_file = config.logging.max_lines_per_file.max(1);
        let timestamps = config.logging.timestamps;

        let capacity = buffer_lines.saturating_mul(4).max(100);
        let (tx, rx) = std_mpsc::sync_channel::<String>(capacity);
        let dropped = Arc::new(AtomicUsize::new(0));
        let dropped_clone = dropped.clone();

        let settings = LogWriterSettings {
            dir,
            buffer_lines,
            flush_interval,
            max_lines_per_file,
            timestamps,
        };

        thread::spawn(move || {
            if let Err(err) = run_log_writer(rx, dropped_clone, settings) {
                error!("Raw logger exited with error: {}", err);
            }
        });

        Ok(Some(Self { tx, dropped }))
    }

    pub fn log_line(&self, line: &str) {
        match self.tx.try_send(line.to_string()) {
            Ok(()) => {}
            Err(std_mpsc::TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(std_mpsc::TrySendError::Disconnected(_)) => {}
        }
    }
}

fn run_log_writer(
    rx: std_mpsc::Receiver<String>,
    dropped: Arc<AtomicUsize>,
    settings: LogWriterSettings,
) -> Result<()> {
    fs::create_dir_all(&settings.dir).context("Failed to create log directory")?;

    let mut writer = open_log_writer(&settings.dir)?;
    let mut buffer: Vec<String> = Vec::with_capacity(settings.buffer_lines);
    let mut lines_written: usize = 0;

    loop {
        match rx.recv_timeout(settings.flush_interval) {
            Ok(line) => {
                buffer.push(line);
                if buffer.len() >= settings.buffer_lines {
                    flush_log_buffer(
                        &mut writer,
                        &mut buffer,
                        &mut lines_written,
                        &settings,
                    )?;
                }
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                if !buffer.is_empty() {
                    flush_log_buffer(
                        &mut writer,
                        &mut buffer,
                        &mut lines_written,
                        &settings,
                    )?;
                }
                report_dropped(&dropped);
            }
            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                if !buffer.is_empty() {
                    flush_log_buffer(
                        &mut writer,
                        &mut buffer,
                        &mut lines_written,
                        &settings,
                    )?;
                }
                report_dropped(&dropped);
                writer.flush().ok();
                break;
            }
        }
    }

    Ok(())
}

fn report_dropped(dropped: &AtomicUsize) {
    let count = dropped.swap(0, Ordering::Relaxed);
    if count > 0 {
        tracing::warn!("Raw logger dropped {} lines (buffer full)", count);
    }
}

fn open_log_writer(dir: &Path) -> Result<BufWriter<std::fs::File>> {
    let timestamp = Local::now().format("%Y-%m-%d-%H-%M-%S");
    let path = dir.join(format!("{}.xml", timestamp));
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("Failed to open raw log file {:?}", path))?;
    info!("Raw log file: {:?}", path);
    Ok(BufWriter::new(file))
}

fn flush_log_buffer(
    writer: &mut BufWriter<std::fs::File>,
    buffer: &mut Vec<String>,
    lines_written: &mut usize,
    settings: &LogWriterSettings,
) -> Result<()> {
    for line in buffer.drain(..) {
        let output = if settings.timestamps {
            let timestamp = Local::now().format("%H:%M:%S");
            format!("{} {}", timestamp, line)
        } else {
            line
        };
        writeln!(writer, "{}", output)?;
        *lines_written += 1;

        if *lines_written >= settings.max_lines_per_file {
            writer.flush()?;
            *writer = open_log_writer(&settings.dir)?;
            *lines_written = 0;
        }
    }

    writer.flush()?;
    Ok(())
}

impl DirectConnectConfig {
    /// Convert game name to game code
    fn game_name_to_code(name: &str) -> &'static str {
        match name.to_lowercase().as_str() {
            // GemStone IV
            "prime" | "gs3" => "GS3",
            "platinum" | "gsx" => "GSX",
            "shattered" | "gsf" => "GSF",
            "test" | "gst" => "GST",
            // DragonRealms
            "dr" | "drprime" => "DR",
            "drplatinum" | "drx" => "DRX",
            "drfallen" | "drf" => "DRF",
            "drtest" | "drt" => "DRT",
            _ => "GS3", // Default to GemStone IV prime
        }
    }

    /// Build DirectConnectConfig from CLI arguments and config
    ///
    /// Resolution order for each field:
    /// - account: CLI --account → config.connection.account → error
    /// - password: CLI --password → config.connection.password → prompt user
    /// - character: CLI --character → config.connection.character → error
    /// - game: CLI --game → config.connection.game → "prime" (default)
    pub fn from_cli(
        direct_enabled: bool,
        direct_account: Option<String>,
        direct_password: Option<String>,
        direct_character: Option<String>,
        character_fallback: Option<String>,
        direct_game: Option<&str>,
        config: &crate::config::Config,
    ) -> Result<Option<Self>> {
        if !direct_enabled {
            return Ok(None);
        }

        // Account: CLI → config → error
        let account = direct_account
            .or_else(|| config.connection.account.clone())
            .context(
                "Account required for --direct. Use --account or set connection.account in config",
            )?;

        // Password: CLI → config → prompt
        let password = match direct_password.or_else(|| config.connection.password.clone()) {
            Some(pwd) => pwd,
            None => {
                let prompt = format!("Password for account {}: ", account);
                rpassword::prompt_password(prompt).context("Failed to read password")?
            }
        };

        // Character: CLI → fallback → config → error
        let character = direct_character
            .or(character_fallback)
            .or_else(|| config.connection.character.clone())
            .context(
                "Character required for --direct. Use --character or set connection.character in config",
            )?;

        // Game: CLI → config → "prime" default
        let game_code = if let Some(game) = direct_game {
            game.to_string()
        } else if let Some(ref game_name) = config.connection.game {
            Self::game_name_to_code(game_name).to_string()
        } else {
            "GS3".to_string() // Default to prime
        };

        let data_dir = crate::config::Config::base_dir()?;

        Ok(Some(Self {
            account,
            password,
            character,
            game_code,
            data_dir,
        }))
    }
}

/// Direct connector that authenticates via eAccess and establishes the game socket.
pub struct DirectConnection;

impl LichConnection {
    /// Connect to Lich, spawn read loop, and forward commands supplied via the provided channel.
    pub async fn start(
        host: &str,
        port: u16,
        server_tx: mpsc::UnboundedSender<ServerMessage>,
        command_rx: mpsc::UnboundedReceiver<String>,
        raw_logger: Option<RawLogger>,
    ) -> Result<()> {
        info!("Connecting to Lich at {}:{}...", host, port);

        let mut stream = TcpStream::connect(format!("{}:{}", host, port))
            .await
            .context("Failed to connect to Lich")?;

        info!("Connected successfully");

        send_pid_handshake(&mut stream).await?;

        run_stream(stream, server_tx, command_rx, raw_logger).await
    }
}

impl DirectConnection {
    pub async fn start(
        config: DirectConnectConfig,
        server_tx: mpsc::UnboundedSender<ServerMessage>,
        command_rx: mpsc::UnboundedReceiver<String>,
        raw_logger: Option<RawLogger>,
    ) -> Result<()> {
        let DirectConnectConfig {
            account,
            password,
            character,
            game_code,
            data_dir,
        } = config;

        info!(
            "Authenticating account '{}' for character '{}' via eAccess...",
            account, character
        );

        let ticket = tokio::task::spawn_blocking(move || {
            eaccess::authenticate(&account, &password, &character, &game_code, &data_dir)
        })
        .await?
        .context("Failed to authenticate with eAccess")?;

        info!(
            "Authentication successful (world: {}, host: {}:{})",
            ticket.game, ticket.game_host, ticket.game_port
        );

        let (host, port) = fix_game_host_port(&ticket.game_host, ticket.game_port);
        info!("Connecting directly to {}:{}...", host, port);
        let mut stream = TcpStream::connect(format!("{}:{}", host, port))
            .await
            .context("Failed to connect to game server")?;

        send_direct_handshake(&mut stream, &ticket).await?;

        run_stream(stream, server_tx, command_rx, raw_logger).await
    }
}

async fn run_stream(
    stream: TcpStream,
    server_tx: mpsc::UnboundedSender<ServerMessage>,
    mut command_rx: mpsc::UnboundedReceiver<String>,
    raw_logger: Option<RawLogger>,
) -> Result<()> {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);

    let _ = server_tx.send(ServerMessage::Connected);

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
                    let line = line.trim_end_matches(['\r', '\n']);
                    if let Some(logger) = &raw_logger {
                        logger.log_line(line);
                    }
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

    let _ = async {
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

    let _ = read_handle.await;

    Ok(())
}

async fn send_pid_handshake(stream: &mut TcpStream) -> Result<()> {
    let pid = std::process::id();
    let msg = format!("SET_FRONTEND_PID {}\n", pid);
    stream.write_all(msg.as_bytes()).await?;
    stream.flush().await?;
    debug!("Sent frontend PID: {}", pid);
    Ok(())
}

async fn send_direct_handshake(
    stream: &mut TcpStream,
    ticket: &eaccess::LaunchTicket,
) -> Result<()> {
    let key = ticket.key.trim();
    stream.write_all(key.as_bytes()).await?;
    stream.write_all(b"\n").await?;

    let fe_string = format!(
        "/FE:WIZARD /VERSION:1.0.1.22 /P:{} /XML",
        std::env::consts::OS
    );
    stream.write_all(fe_string.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

    for _ in 0..2 {
        stream.write_all(b"<c>\n").await?;
        stream.flush().await?;
        sleep(TokioDuration::from_millis(300)).await;
    }

    Ok(())
}

fn fix_game_host_port(host: &str, port: u16) -> (String, u16) {
    let lowered = host.to_ascii_lowercase();
    match (lowered.as_str(), port) {
        ("gs-plat.simutronics.net", 10121) => ("storm.gs4.game.play.net".to_string(), 10124),
        ("gs3.simutronics.net", 4900) => ("storm.gs4.game.play.net".to_string(), 10024),
        ("gs4.simutronics.net", 10321) => ("storm.gs4.game.play.net".to_string(), 10324),
        ("prime.dr.game.play.net", 4901) => ("dr.simutronics.net".to_string(), 11024),
        _ => (host.to_string(), port),
    }
}

mod eaccess {
    use anyhow::{anyhow, bail, Context, Result};
    use openssl::ssl::{SslConnector, SslMethod, SslStream, SslVerifyMode};
    use openssl::x509::X509;
    use std::collections::HashMap;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::Path;

    const HOST: &str = "eaccess.play.net";
    const PORT: u16 = 7910;
    const CERT_FILENAME: &str = "simu.pem";

    #[derive(Clone, Debug)]
    pub struct LaunchTicket {
        pub key: String,
        pub game_host: String,
        pub game_port: u16,
        pub game: String,
        pub character: String,
    }

    pub fn authenticate(
        account: &str,
        password: &str,
        character: &str,
        game_code: &str,
        data_dir: &Path,
    ) -> Result<LaunchTicket> {
        let cert_path = data_dir.join(CERT_FILENAME);
        ensure_certificate(&cert_path)?;

        tracing::debug!("TLS handshake to eAccess starting (cert: {:?})", cert_path);
        let mut stream = match connect_with_cert(&cert_path) {
            Ok(stream) => {
                tracing::debug!("TLS handshake to eAccess succeeded");
                stream
            }
            Err(err) => {
                tracing::warn!(error = ?err, "Handshake failed, refreshing stored cert");
                download_certificate(&cert_path)?;
                let stream = connect_with_cert(&cert_path)?;
                tracing::debug!("TLS handshake succeeded after refreshing cert");
                stream
            }
        };

        send_line(&mut stream, "K")?;
        let hash_key = read_response(&mut stream)?;
        let encoded_password = obfuscate_password(password, hash_key.trim());

        send_login_payload(&mut stream, account, &encoded_password)?;
        let auth_response = read_response(&mut stream)?;

        if !auth_response.contains("KEY") {
            bail!(
                "Authentication failed for account {}: {}",
                account,
                auth_response.trim()
            );
        }

        send_line(&mut stream, &format!("F\t{}", game_code))?;
        read_response(&mut stream)?; // Subscription tier
        send_line(&mut stream, &format!("G\t{}", game_code))?;
        read_response(&mut stream)?; // Game status
        send_line(&mut stream, &format!("P\t{}", game_code))?;
        read_response(&mut stream)?; // Billing info

        send_line(&mut stream, "C")?;
        let characters_response = read_response(&mut stream)?;
        let char_code = parse_character_code(&characters_response, character).ok_or_else(|| {
            anyhow!(
                "Character '{}' not found in account '{}'",
                character,
                account
            )
        })?;

        send_line(&mut stream, &format!("L\t{}\tSTORM", char_code))?;
        let launch_response = read_response(&mut stream)?;
        parse_launch_response(&launch_response)
    }

    fn ensure_certificate(path: &Path) -> Result<()> {
        if path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        download_certificate(path)
    }

    fn download_certificate(path: &Path) -> Result<()> {
        // Create permissive connector to download cert
        let mut connector = SslConnector::builder(SslMethod::tls_client())?;
        connector.set_verify(SslVerifyMode::NONE);

        let stream = TcpStream::connect((HOST, PORT))?;
        stream.set_nodelay(true)?;
        let connector = connector.build();
        let tls_stream = connector.connect(HOST, stream)?;

        // Get peer certificate and save as PEM
        let cert = tls_stream
            .ssl()
            .peer_certificate()
            .ok_or_else(|| anyhow!("Server did not provide a certificate"))?;

        let pem = cert.to_pem()?;
        fs::write(path, pem).context("Failed to save certificate")?;
        Ok(())
    }

    fn connect_with_cert(cert_path: &Path) -> Result<SslStream<TcpStream>> {
        let cert_data = fs::read(cert_path).context("Failed to read stored certificate")?;
        let stored_cert = X509::from_pem(&cert_data)
            .context("Invalid PEM certificate")?;

        // Create connector with the stored certificate
        // Configure like Ruby's OpenSSL - allow both TLS 1.2 and 1.3
        let mut connector = SslConnector::builder(SslMethod::tls_client())?;

        // Add our stored cert as a trusted root
        connector.cert_store_mut().add_cert(stored_cert.clone())?;

        // Let OpenSSL negotiate - server will pick TLS 1.2, but we need to offer 1.3 too
        // (Server expects clients to advertise TLS 1.3 cipher suites even if it uses 1.2)

        // Disable session caching to avoid sending a Session ID (match Lich's empty Session ID)
        use openssl::ssl::SslSessionCacheMode;
        connector.set_session_cache_mode(SslSessionCacheMode::OFF);

        // Disable automatic verification - we do manual verification below (like Lich)
        // The cert has no hostname, so automatic verification would fail
        connector.set_verify(SslVerifyMode::NONE);

        let connector = connector.build();
        let stream = TcpStream::connect((HOST, PORT)).context("Failed to open TLS socket")?;
        stream.set_nodelay(true)?;

        // Disable SNI - Ruby doesn't send it by default for IP-based connections
        let mut config = connector.configure()?;
        config.set_use_server_name_indication(false);
        config.set_verify_hostname(false);

        let tls_stream = config
            .connect("", stream)
            .context("TLS handshake with eAccess failed")?;

        // Log TLS details
        tracing::debug!("TLS version: {:?}, Cipher: {:?}",
            tls_stream.ssl().version_str(),
            tls_stream.ssl().current_cipher().map(|c| c.name()));

        // Manually verify the peer certificate matches our stored one (like Lich's verify_pem)
        let peer_cert = tls_stream
            .ssl()
            .peer_certificate()
            .ok_or_else(|| anyhow!("Server did not provide a certificate"))?;

        let peer_pem = peer_cert.to_pem()?;
        if peer_pem != cert_data {
            tracing::warn!("Certificate mismatch - refreshing stored certificate");
            download_certificate(cert_path)?;
        }

        Ok(tls_stream)
    }

    fn send_line(stream: &mut SslStream<TcpStream>, line: &str) -> Result<()> {
        // Match Ruby's puts - sends string with newline in a SINGLE write
        // Build the complete message with newline, then write it all at once
        // to ensure it goes out as a single TLS record
        let mut message = Vec::with_capacity(line.len() + 1);
        message.extend_from_slice(line.as_bytes());
        message.push(b'\n');

        stream.write_all(&message)?;
        stream.flush()?;
        Ok(())
    }

    fn send_login_payload(
        stream: &mut SslStream<TcpStream>,
        account: &str,
        encoded_password: &[u8],
    ) -> Result<()> {
        // Build entire payload in memory first, then send as single write
        // to ensure it goes out as a single TLS record
        let mut payload = Vec::new();
        payload.extend_from_slice(b"A\t");
        payload.extend_from_slice(account.as_bytes());
        payload.extend_from_slice(b"\t");
        payload.extend_from_slice(encoded_password);
        payload.extend_from_slice(b"\n");

        stream.write_all(&payload)?;
        stream.flush()?;
        Ok(())
    }

    fn read_response(stream: &mut SslStream<TcpStream>) -> Result<String> {
        // Match Ruby's conn.sysread(PACKET_SIZE) behavior - read up to 8192 bytes in one blocking call
        const PACKET_SIZE: usize = 8192;
        let mut buf = vec![0u8; PACKET_SIZE];

        let bytes_read = stream.read(&mut buf)?;

        if bytes_read == 0 {
            return Ok(String::new());
        }

        // Truncate to actual bytes read
        buf.truncate(bytes_read);

        let response = String::from_utf8(buf).context("Response was not valid UTF-8")?;
        Ok(response)
    }

    fn obfuscate_password(password: &str, hash_key: &str) -> Vec<u8> {
        password
            .bytes()
            .zip(hash_key.bytes())
            .map(|(pwd, hash)| {
                // Match Ruby's behavior: ((pwd - 32) ^ hash) + 32
                // where the subtraction can go negative
                let pwd_adjusted = (pwd as i32) - 32;
                let xor_result = pwd_adjusted ^ (hash as i32);
                let final_result = xor_result + 32;
                final_result as u8
            })
            .collect()
    }

    fn parse_character_code(response: &str, target: &str) -> Option<String> {
        let trimmed = response.trim();
        let tokens: Vec<&str> = trimmed.split('\t').collect();
        if tokens.len() <= 5 || tokens.first().copied()? != "C" {
            return None;
        }
        let mut index = 5;
        while index + 1 < tokens.len() {
            let code = tokens[index];
            let name = tokens[index + 1];
            if name.eq_ignore_ascii_case(target) {
                return Some(code.to_string());
            }
            index += 2;
        }
        None
    }

    fn parse_launch_response(response: &str) -> Result<LaunchTicket> {
        let trimmed = response.trim();
        if !trimmed.starts_with('L') {
            bail!("Unexpected response to launch command: {}", trimmed);
        }

        let payload = trimmed
            .strip_prefix("L\t")
            .unwrap_or(trimmed)
            .strip_prefix("OK\t")
            .unwrap_or(trimmed);

        let mut values = HashMap::new();
        for pair in payload.split('\t') {
            if let Some((key, value)) = pair.split_once('=') {
                values.insert(key.to_uppercase(), value.to_string());
            }
        }

        let key = values
            .remove("KEY")
            .context("Launch response missing KEY")?;
        let host = values
            .remove("GAMEHOST")
            .context("Launch response missing GAMEHOST")?;
        let port = values
            .remove("GAMEPORT")
            .context("Launch response missing GAMEPORT")?
            .parse::<u16>()
            .context("Invalid GAMEPORT value")?;
        let game = values.get("GAME").cloned().unwrap_or_default();
        let character = values
            .get("CHARACTER")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        Ok(LaunchTicket {
            key,
            game_host: host,
            game_port: port,
            game,
            character,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // ========== obfuscate_password tests ==========

        #[test]
        fn test_obfuscate_password_basic() {
            // Test that password obfuscation produces expected output
            let password = "test";
            let hash_key = "ABCD";
            let result = obfuscate_password(password, hash_key);

            // Verify length matches password length
            assert_eq!(result.len(), password.len());

            // Verify the algorithm: ((pwd - 32) ^ hash) + 32
            let expected: Vec<u8> = password
                .bytes()
                .zip(hash_key.bytes())
                .map(|(p, h)| (((p as i32 - 32) ^ h as i32) + 32) as u8)
                .collect();
            assert_eq!(result, expected);
        }

        #[test]
        fn test_obfuscate_password_empty() {
            let result = obfuscate_password("", "ABCD");
            assert!(result.is_empty());
        }

        #[test]
        fn test_obfuscate_password_shorter_hash() {
            // When hash is shorter than password, zip stops at shorter
            let password = "password";
            let hash_key = "AB";
            let result = obfuscate_password(password, hash_key);
            assert_eq!(result.len(), 2); // Only 2 chars processed
        }

        #[test]
        fn test_obfuscate_password_special_chars() {
            let password = "P@ss!23";
            let hash_key = "ABCDEFG";
            let result = obfuscate_password(password, hash_key);
            assert_eq!(result.len(), 7);
        }

        #[test]
        fn test_obfuscate_password_deterministic() {
            // Same inputs should always produce same output
            let password = "mypassword";
            let hash_key = "0123456789";
            let result1 = obfuscate_password(password, hash_key);
            let result2 = obfuscate_password(password, hash_key);
            assert_eq!(result1, result2);
        }

        // ========== parse_character_code tests ==========

        #[test]
        fn test_parse_character_code_found() {
            let response = "C\t5\t0\t0\t0\tABC123\tMyChar\tDEF456\tOtherChar";
            let result = parse_character_code(response, "MyChar");
            assert_eq!(result, Some("ABC123".to_string()));
        }

        #[test]
        fn test_parse_character_code_case_insensitive() {
            let response = "C\t5\t0\t0\t0\tABC123\tMyChar\tDEF456\tOtherChar";
            let result = parse_character_code(response, "mychar");
            assert_eq!(result, Some("ABC123".to_string()));
        }

        #[test]
        fn test_parse_character_code_second_character() {
            let response = "C\t5\t0\t0\t0\tABC123\tFirstChar\tDEF456\tSecondChar";
            let result = parse_character_code(response, "SecondChar");
            assert_eq!(result, Some("DEF456".to_string()));
        }

        #[test]
        fn test_parse_character_code_not_found() {
            let response = "C\t5\t0\t0\t0\tABC123\tMyChar";
            let result = parse_character_code(response, "NonExistent");
            assert_eq!(result, None);
        }

        #[test]
        fn test_parse_character_code_invalid_prefix() {
            let response = "X\t5\t0\t0\t0\tABC123\tMyChar";
            let result = parse_character_code(response, "MyChar");
            assert_eq!(result, None);
        }

        #[test]
        fn test_parse_character_code_insufficient_fields() {
            let response = "C\t1\t2\t3";
            let result = parse_character_code(response, "MyChar");
            assert_eq!(result, None);
        }

        #[test]
        fn test_parse_character_code_whitespace_trimmed() {
            let response = "  C\t5\t0\t0\t0\tABC123\tMyChar  \n";
            let result = parse_character_code(response, "MyChar");
            assert_eq!(result, Some("ABC123".to_string()));
        }

        // ========== parse_launch_response tests ==========

        #[test]
        fn test_parse_launch_response_valid() {
            let response = "L\tOK\tKEY=abc123\tGAMEHOST=game.server.net\tGAMEPORT=4900\tGAME=GS3\tCHARACTER=TestChar";
            let result = parse_launch_response(response).unwrap();
            assert_eq!(result.key, "abc123");
            assert_eq!(result.game_host, "game.server.net");
            assert_eq!(result.game_port, 4900);
            assert_eq!(result.game, "GS3");
            assert_eq!(result.character, "TestChar");
        }

        #[test]
        fn test_parse_launch_response_minimal() {
            // Only required fields
            let response = "L\tOK\tKEY=xyz\tGAMEHOST=host\tGAMEPORT=1234";
            let result = parse_launch_response(response).unwrap();
            assert_eq!(result.key, "xyz");
            assert_eq!(result.game_host, "host");
            assert_eq!(result.game_port, 1234);
            assert!(result.game.is_empty());
            assert_eq!(result.character, "unknown");
        }

        #[test]
        fn test_parse_launch_response_case_insensitive_keys() {
            let response = "L\tOK\tkey=abc\tgamehost=host\tgameport=5000";
            let result = parse_launch_response(response).unwrap();
            assert_eq!(result.key, "abc");
            assert_eq!(result.game_host, "host");
            assert_eq!(result.game_port, 5000);
        }

        #[test]
        fn test_parse_launch_response_missing_key() {
            let response = "L\tOK\tGAMEHOST=host\tGAMEPORT=1234";
            let result = parse_launch_response(response);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("KEY"));
        }

        #[test]
        fn test_parse_launch_response_missing_host() {
            let response = "L\tOK\tKEY=abc\tGAMEPORT=1234";
            let result = parse_launch_response(response);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("GAMEHOST"));
        }

        #[test]
        fn test_parse_launch_response_missing_port() {
            let response = "L\tOK\tKEY=abc\tGAMEHOST=host";
            let result = parse_launch_response(response);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("GAMEPORT"));
        }

        #[test]
        fn test_parse_launch_response_invalid_port() {
            let response = "L\tOK\tKEY=abc\tGAMEHOST=host\tGAMEPORT=notanumber";
            let result = parse_launch_response(response);
            assert!(result.is_err());
        }

        #[test]
        fn test_parse_launch_response_invalid_prefix() {
            let response = "X\tOK\tKEY=abc\tGAMEHOST=host\tGAMEPORT=1234";
            let result = parse_launch_response(response);
            assert!(result.is_err());
        }

        #[test]
        fn test_parse_launch_response_whitespace() {
            let response = "  L\tOK\tKEY=abc\tGAMEHOST=host\tGAMEPORT=1234  \n";
            let result = parse_launch_response(response).unwrap();
            assert_eq!(result.key, "abc");
        }

        // ========== LaunchTicket tests ==========

        #[test]
        fn test_launch_ticket_clone() {
            let ticket = LaunchTicket {
                key: "test_key".to_string(),
                game_host: "test_host".to_string(),
                game_port: 1234,
                game: "GS3".to_string(),
                character: "TestChar".to_string(),
            };
            let cloned = ticket.clone();
            assert_eq!(ticket.key, cloned.key);
            assert_eq!(ticket.game_host, cloned.game_host);
            assert_eq!(ticket.game_port, cloned.game_port);
        }

        #[test]
        fn test_launch_ticket_debug() {
            let ticket = LaunchTicket {
                key: "secret".to_string(),
                game_host: "host".to_string(),
                game_port: 4900,
                game: "GS3".to_string(),
                character: "Char".to_string(),
            };
            let debug_str = format!("{:?}", ticket);
            assert!(debug_str.contains("LaunchTicket"));
            assert!(debug_str.contains("host"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== fix_game_host_port tests ==========

    #[test]
    fn test_fix_game_host_port_gs_plat() {
        let (host, port) = fix_game_host_port("gs-plat.simutronics.net", 10121);
        assert_eq!(host, "storm.gs4.game.play.net");
        assert_eq!(port, 10124);
    }

    #[test]
    fn test_fix_game_host_port_gs3() {
        let (host, port) = fix_game_host_port("gs3.simutronics.net", 4900);
        assert_eq!(host, "storm.gs4.game.play.net");
        assert_eq!(port, 10024);
    }

    #[test]
    fn test_fix_game_host_port_gs4() {
        let (host, port) = fix_game_host_port("gs4.simutronics.net", 10321);
        assert_eq!(host, "storm.gs4.game.play.net");
        assert_eq!(port, 10324);
    }

    #[test]
    fn test_fix_game_host_port_dr() {
        let (host, port) = fix_game_host_port("prime.dr.game.play.net", 4901);
        assert_eq!(host, "dr.simutronics.net");
        assert_eq!(port, 11024);
    }

    #[test]
    fn test_fix_game_host_port_unknown() {
        let (host, port) = fix_game_host_port("unknown.server.net", 1234);
        assert_eq!(host, "unknown.server.net");
        assert_eq!(port, 1234);
    }

    #[test]
    fn test_fix_game_host_port_case_insensitive() {
        let (host, port) = fix_game_host_port("GS3.SIMUTRONICS.NET", 4900);
        assert_eq!(host, "storm.gs4.game.play.net");
        assert_eq!(port, 10024);
    }

    #[test]
    fn test_fix_game_host_port_wrong_port_for_host() {
        // GS3 host but wrong port - should not match
        let (host, port) = fix_game_host_port("gs3.simutronics.net", 9999);
        assert_eq!(host, "gs3.simutronics.net");
        assert_eq!(port, 9999);
    }

    // ========== ServerMessage tests ==========

    #[test]
    fn test_server_message_text() {
        let msg = ServerMessage::Text("hello".to_string());
        if let ServerMessage::Text(s) = msg {
            assert_eq!(s, "hello");
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn test_server_message_connected() {
        let msg = ServerMessage::Connected;
        assert!(matches!(msg, ServerMessage::Connected));
    }

    #[test]
    fn test_server_message_disconnected() {
        let msg = ServerMessage::Disconnected;
        assert!(matches!(msg, ServerMessage::Disconnected));
    }

    #[test]
    fn test_server_message_clone() {
        let msg = ServerMessage::Text("test".to_string());
        let cloned = msg.clone();
        if let ServerMessage::Text(s) = cloned {
            assert_eq!(s, "test");
        }
    }

    #[test]
    fn test_server_message_debug() {
        let msg = ServerMessage::Text("data".to_string());
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("Text"));
        assert!(debug_str.contains("data"));
    }
}
