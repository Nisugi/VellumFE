use crossterm::terminal;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{self, Write};
use tracing;

#[cfg(windows)]
#[derive(Debug, Clone)]
struct MonitorInfo {
    device: String,
    rect_left: i32,
    rect_top: i32,
    rect_right: i32,
    rect_bottom: i32,
}

#[cfg(windows)]
#[derive(Debug, Clone)]
struct TargetMonitor {
    device: String,
    rect_left: i32,
    rect_top: i32,
    rect_right: i32,
    rect_bottom: i32,
    original_left: i32,
    original_top: i32,
}

#[cfg(unix)]
use std::io::Read;
#[cfg(unix)]
use std::time::{Duration, Instant};

/// Represents the terminal window's position and size on screen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalPosition {
    /// X coordinate on screen (pixels from left edge) - optional, may not be available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    /// Y coordinate on screen (pixels from top edge) - optional, may not be available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    /// Window width in pixels - optional, may not be available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    /// Window height in pixels - optional, may not be available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    /// Terminal width in columns - this is the most reliable value
    pub cols: u16,
    /// Terminal height in rows - this is the most reliable value
    pub rows: u16,
    /// Monitor device name (Windows only, e.g., \\.\DISPLAY2)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor_device: Option<String>,
    /// Monitor rectangle (Windows only) - to restore relative position on a specific monitor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor_left: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor_top: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor_right: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor_bottom: Option<i32>,
}

impl TerminalPosition {
    /// Query the current terminal window position and size
    /// This must be called when NOT in raw mode (i.e., before UI starts or after UI stops)
    /// Returns None if terminal doesn't support queries or timeout occurs
    pub fn query() -> Option<Self> {
        // Get terminal size in characters (always reliable)
        let (cols, rows) = match terminal::size() {
            Ok(size) => size,
            Err(e) => {
                tracing::warn!("Failed to get terminal size: {}", e);
                return None;
            }
        };

        tracing::debug!("Terminal size: {}x{}", cols, rows);

        // Try to query position and pixel size using CSI t sequences
        // This requires reading raw stdin, so we do it carefully
        match Self::query_extended_info() {
            Ok((x, y, width, height)) => {
                #[cfg(windows)]
                let monitor_info = Self::query_monitor_info_windows();

                tracing::debug!(
                    "Successfully queried terminal position and size: x={}, y={}, width={}, height={}",
                    x, y, width, height
                );
                Some(TerminalPosition {
                    x: Some(x),
                    y: Some(y),
                    width: Some(width),
                    height: Some(height),
                    cols,
                    rows,
                    #[cfg(windows)]
                    monitor_device: monitor_info.as_ref().map(|m| m.device.clone()),
                    #[cfg(windows)]
                    monitor_left: monitor_info.as_ref().map(|m| m.rect_left),
                    #[cfg(windows)]
                    monitor_top: monitor_info.as_ref().map(|m| m.rect_top),
                    #[cfg(windows)]
                    monitor_right: monitor_info.as_ref().map(|m| m.rect_right),
                    #[cfg(windows)]
                    monitor_bottom: monitor_info.as_ref().map(|m| m.rect_bottom),
                    #[cfg(not(windows))]
                    monitor_device: None,
                    #[cfg(not(windows))]
                    monitor_left: None,
                    #[cfg(not(windows))]
                    monitor_top: None,
                    #[cfg(not(windows))]
                    monitor_right: None,
                    #[cfg(not(windows))]
                    monitor_bottom: None,
                })
            }
            Err(e) => {
                tracing::debug!("Could not query extended terminal info ({}), using size only", e);
                // Return position with just size info - this is still useful
                Some(TerminalPosition {
                    x: None,
                    y: None,
                    width: None,
                    height: None,
                    cols,
                    rows,
                    monitor_device: None,
                    monitor_left: None,
                    monitor_top: None,
                    monitor_right: None,
                    monitor_bottom: None,
                })
            }
        }
    }

    /// Query extended terminal info (position and pixel size) using CSI t sequences
    /// Returns (x, y, width, height) or error
    fn query_extended_info() -> io::Result<(i32, i32, i32, i32)> {
        #[cfg(windows)]
        {
            // On Windows we rely on Win32 APIs instead of CSI queries. Sending
            // the escape sequences without reading the responses leaves bytes
            // in the input buffer, which show up at the PowerShell prompt as
            // "[4;...t" after exit.
            Self::read_responses_windows()
        }
        #[cfg(unix)]
        {
            let mut stdout = io::stdout();

            // Send queries using xterm control sequences
            // CSI 13 t - Report window position (responds: CSI 3 ; x ; y t)
            // CSI 14 t - Report window size in pixels (responds: CSI 4 ; height ; width t)
            write!(stdout, "\x1b[13t\x1b[14t")?;
            stdout.flush()?;

            // We need to read raw bytes from stdin with a timeout
            // This is platform-specific
            Self::read_responses_unix()
        }
    }

    #[cfg(windows)]
    fn read_responses_windows() -> io::Result<(i32, i32, i32, i32)> {
        use winapi::um::wincon::GetConsoleWindow;
        use winapi::um::winuser::GetWindowRect;
        use winapi::shared::windef::RECT;

        unsafe {
            // Get the console window handle
            let hwnd = GetConsoleWindow();
            if hwnd.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Could not get console window handle",
                ));
            }

            // Get window rectangle (position and size)
            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "GetWindowRect failed",
                ));
            }

            // RECT contains: left, top, right, bottom (in screen coordinates)
            let x = rect.left;
            let y = rect.top;
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;

            tracing::debug!("Windows API query: x={}, y={}, width={}, height={}", x, y, width, height);

            Ok((x, y, width, height))
        }
    }

    #[cfg(unix)]
    fn read_responses_unix() -> io::Result<(i32, i32, i32, i32)> {
        use std::os::unix::io::AsRawFd;
        use nix::sys::select::{select, FdSet};
        use nix::sys::time::{TimeVal, TimeValLike};
        use std::os::fd::{AsFd, BorrowedFd};

        let mut stdin = io::stdin();
        let fd = stdin.as_raw_fd();

        let mut buffer = Vec::new();
        let timeout = Duration::from_millis(200);
        let start = Instant::now();

        while start.elapsed() < timeout && buffer.len() < 512 {
            let remaining = timeout.saturating_sub(start.elapsed());
            let millis = remaining.as_millis() as i64;
            let seconds = millis / 1000;
            let microseconds = (millis % 1000) * 1000;

            // TimeVal signature differs by platform: (i64, i64) on Linux, (i64, i32) on macOS
            #[cfg(target_os = "linux")]
            let mut tv = TimeVal::new(seconds, microseconds);
            #[cfg(not(target_os = "linux"))]
            let mut tv = TimeVal::new(seconds, microseconds as i32);

            let mut fds = FdSet::new();
            let borrowed_fd = unsafe { BorrowedFd::borrow_raw(fd) };
            fds.insert(borrowed_fd);

            match select(fd + 1, Some(&mut fds), None, None, Some(&mut tv)) {
                Ok(n) if n > 0 => {
                    let mut byte = [0u8; 1];
                    if stdin.read_exact(&mut byte).is_ok() {
                        buffer.push(byte[0]);
                    }
                }
                _ => break,
            }
        }

        // Parse CSI responses from buffer
        Self::parse_csi_responses(&buffer)
    }

    /// Parse CSI responses from raw byte buffer
    /// Looking for: ESC [ 3 ; x ; y t and ESC [ 4 ; height ; width t
    fn parse_csi_responses(buffer: &[u8]) -> io::Result<(i32, i32, i32, i32)> {
        let s = String::from_utf8_lossy(buffer);
        tracing::debug!("Raw CSI response buffer: {:?}", s);

        let mut x = None;
        let mut y = None;
        let mut width = None;
        let mut height = None;

        // Parse position response: ESC [ 3 ; x ; y t
        if let Some(pos) = s.find("\x1b[3;") {
            if let Some(end) = s[pos..].find('t') {
                let params = &s[pos + 4..pos + end];
                let parts: Vec<&str> = params.split(';').collect();
                if parts.len() == 2 {
                    x = parts[0].parse().ok();
                    y = parts[1].parse().ok();
                }
            }
        }

        // Parse size response: ESC [ 4 ; height ; width t
        if let Some(pos) = s.find("\x1b[4;") {
            if let Some(end) = s[pos..].find('t') {
                let params = &s[pos + 4..pos + end];
                let parts: Vec<&str> = params.split(';').collect();
                if parts.len() == 2 {
                    height = parts[0].parse().ok();
                    width = parts[1].parse().ok();
                }
            }
        }

        match (x, y, width, height) {
            (Some(x), Some(y), Some(w), Some(h)) => Ok((x, y, w, h)),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Failed to parse terminal position responses",
            )),
        }
    }

    /// Apply this terminal position/size
    /// Uses platform-specific APIs to move and resize the terminal window
    pub fn apply(&self) -> io::Result<()> {
        tracing::debug!("Applying terminal position: {:?}", self);

        #[cfg(windows)]
        {
            self.apply_windows()
        }
        #[cfg(unix)]
        {
            self.apply_unix()
        }
    }

    #[cfg(windows)]
    fn apply_windows(&self) -> io::Result<()> {
        use winapi::um::wincon::GetConsoleWindow;
        use winapi::um::winuser::{
            SetWindowPos, SWP_NOZORDER, SWP_NOACTIVATE, ShowWindow, SW_RESTORE, MoveWindow, GetWindowRect,
        };
        use winapi::shared::windef::RECT;

        unsafe {
            if Self::is_windows_terminal_host() {
                tracing::info!("Skipping terminal reposition: Windows Terminal/ConPTY host detected");
                return Ok(());
            }

            // Get the console window handle
            let hwnd = GetConsoleWindow();
            if hwnd.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Could not get console window handle",
                ));
            }

            // If we have position and size, apply them
            if let (Some(saved_x), Some(saved_y), Some(width), Some(height)) = (self.x, self.y, self.width, self.height) {
                // Start with the raw saved coordinates
                let mut target_x = saved_x;
                let mut target_y = saved_y;

                // If we have monitor info, align to the saved monitor using relative offset
                if let Some(target) = self.resolve_target_monitor() {
                    let dx = saved_x - target.original_left;
                    let dy = saved_y - target.original_top;
                    let unclamped_x = target.rect_left + dx;
                    let unclamped_y = target.rect_top + dy;
                    let max_x = (target.rect_right - width).max(target.rect_left);
                    let max_y = (target.rect_bottom - height).max(target.rect_top);
                    target_x = unclamped_x.clamp(target.rect_left, max_x);
                    target_y = unclamped_y.clamp(target.rect_top, max_y);
                    tracing::info!(
                        "Repositioning relative to monitor '{}': offset=({}, {}), unclamped=({}, {}), clamped=({}, {})",
                        target.device, dx, dy, unclamped_x, unclamped_y, target_x, target_y
                    );
                }

                // Ensure the window is in a normal/restored state before moving (snapped/maximized windows can resist moves across monitors)
                ShowWindow(hwnd, SW_RESTORE);

                // SetWindowPos moves and resizes the window
                // HWND_TOP (0) would change Z-order, so we use SWP_NOZORDER to keep current Z-order
                let result = SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(), // Don't change Z-order
                    target_x,
                    target_y,
                    width,
                    height,
                    SWP_NOZORDER | SWP_NOACTIVATE, // Don't change Z-order or activation state
                );

                if result == 0 {
                    let err = io::Error::last_os_error();
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("SetWindowPos failed: {}", err),
                    ));
                }

                // Verify the window actually moved/resized; if not, try MoveWindow as a fallback (helps across monitors)
                let mut rect: RECT = std::mem::zeroed();
                if GetWindowRect(hwnd, &mut rect) != 0 {
                    let got_w = rect.right - rect.left;
                    let got_h = rect.bottom - rect.top;
                    if rect.left != target_x || rect.top != target_y || got_w != width || got_h != height {
                        tracing::warn!(
                            "SetWindowPos reported success but window is at ({}, {}) {}x{} instead of ({}, {}) {}x{}. Trying MoveWindow fallback.",
                            rect.left, rect.top, got_w, got_h, target_x, target_y, width, height
                        );

                        // First try the target coordinates, then fall back to the raw saved coordinates if needed
                        let mut move_ok = MoveWindow(hwnd, target_x, target_y, width, height, 1);
                        if move_ok == 0 && (target_x != saved_x || target_y != saved_y) {
                            tracing::warn!("MoveWindow at target coords failed; trying raw saved coords ({}, {})", saved_x, saved_y);
                            move_ok = MoveWindow(hwnd, saved_x, saved_y, width, height, 1);
                        }

                        if move_ok == 0 {
                            tracing::warn!("MoveWindow fallback failed; window position may be unchanged");
                        } else {
                            tracing::debug!("MoveWindow fallback applied");
                        }
                    } else {
                        tracing::debug!("Window position verified after SetWindowPos");
                    }
                } else {
                    tracing::debug!("Could not verify window rectangle after SetWindowPos");
                }

                tracing::debug!("Set terminal position to ({}, {}) and size to {}x{}", target_x, target_y, width, height);
            } else {
                tracing::debug!("No position/size data to apply (all fields are None)");
            }

            // Note: Character size (rows/cols) is applied automatically by Windows Console
            // when the window is resized, so we don't need to set it explicitly

            tracing::debug!("Terminal position applied successfully");
            Ok(())
        }
    }

    #[cfg(windows)]
    fn is_windows_terminal_host() -> bool {
        // Windows Terminal/ConPTY sessions typically set WT_SESSION or WT_PROFILE_ID
        env::var("WT_SESSION").is_ok() || env::var("WT_PROFILE_ID").is_ok()
    }

    #[cfg(windows)]
    fn query_monitor_info_windows() -> Option<MonitorInfo> {
        use winapi::um::wincon::GetConsoleWindow;
        use winapi::um::winuser::{MonitorFromWindow, GetMonitorInfoW, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST};

        unsafe {
            let hwnd = GetConsoleWindow();
            if hwnd.is_null() {
                return None;
            }

            let hmonitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if hmonitor.is_null() {
                return None;
            }

            let mut info: MONITORINFOEXW = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

            if GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut _) == 0 {
                return None;
            }

            let device = Self::wchar_to_string(&info.szDevice);
            Some(MonitorInfo {
                device,
                rect_left: info.rcMonitor.left,
                rect_top: info.rcMonitor.top,
                rect_right: info.rcMonitor.right,
                rect_bottom: info.rcMonitor.bottom,
            })
        }
    }

    #[cfg(windows)]
    fn resolve_target_monitor(&self) -> Option<TargetMonitor> {
        use std::ptr;
        use winapi::shared::minwindef::{BOOL, LPARAM, TRUE, FALSE};
        use winapi::shared::windef::HMONITOR;
        use winapi::um::winuser::{EnumDisplayMonitors, GetMonitorInfoW, MONITORINFOEXW};

        let device = self.monitor_device.as_ref()?;
        let original_left = self.monitor_left?;
        let original_top = self.monitor_top?;

        #[cfg(windows)]
        struct EnumData {
            target_device: String,
            found: Option<TargetMonitor>,
            original_left: i32,
            original_top: i32,
        }

        unsafe extern "system" fn enum_proc(
            hmonitor: HMONITOR,
            _hdc: winapi::shared::windef::HDC,
            _lprc: *mut winapi::shared::windef::RECT,
            lparam: LPARAM,
        ) -> BOOL {
            let data = &mut *(lparam as *mut EnumData);

            let mut info: MONITORINFOEXW = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut _) == 0 {
                return TRUE;
            }

            let name = TerminalPosition::wchar_to_string(&info.szDevice);
            if name.eq_ignore_ascii_case(&data.target_device) {
                data.found = Some(TargetMonitor {
                    device: name,
                    rect_left: info.rcMonitor.left,
                    rect_top: info.rcMonitor.top,
                    rect_right: info.rcMonitor.right,
                    rect_bottom: info.rcMonitor.bottom,
                    original_left: data.original_left,
                    original_top: data.original_top,
                });
                return FALSE; // stop enumeration
            }

            TRUE
        }

        let mut data = EnumData {
            target_device: device.clone(),
            found: None,
            original_left,
            original_top,
        };

        unsafe {
            let param = &mut data as *mut EnumData as LPARAM;
            EnumDisplayMonitors(ptr::null_mut(), ptr::null_mut(), Some(enum_proc), param);
        }

        data.found
    }

    #[cfg(windows)]
    fn wchar_to_string(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }

    #[cfg(unix)]
    fn apply_unix(&self) -> io::Result<()> {
        let mut stdout = io::stdout();

        // Apply position if available
        if let (Some(x), Some(y)) = (self.x, self.y) {
            // CSI 3 ; x ; y t - Move window to position x, y
            write!(stdout, "\x1b[3;{};{}t", x, y)?;
            tracing::debug!("Set terminal position to ({}, {})", x, y);
        }

        // Apply pixel size if available
        if let (Some(width), Some(height)) = (self.width, self.height) {
            // CSI 4 ; height ; width t - Resize window to height, width in pixels
            write!(stdout, "\x1b[4;{};{}t", height, width)?;
            tracing::debug!("Set terminal pixel size to {}x{}", width, height);
        }

        // Always apply character size (most reliable)
        // CSI 8 ; rows ; cols t - Resize terminal to rows, cols in characters
        write!(stdout, "\x1b[8;{};{}t", self.rows, self.cols)?;
        tracing::debug!("Set terminal size to {}x{}", self.cols, self.rows);

        stdout.flush()?;

        tracing::debug!("Terminal position applied successfully");
        Ok(())
    }
}
