//! Windows implementation of window positioning using Win32 APIs.
//!
//! Handles both classic ConHost and modern Windows Terminal:
//! - ConHost: Uses GetConsoleWindow() directly
//! - Windows Terminal: Walks process tree to find the WindowsTerminal.exe window

use anyhow::{Context, Result};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows::Win32::System::Console::GetConsoleWindow;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, SetWindowPos, HWND_TOP,
    SWP_NOZORDER,
};

use super::{ScreenInfo, WindowPositioner, WindowRect};

pub struct WindowsPositioner {
    hwnd: HWND,
    is_windows_terminal: bool,
}

impl WindowsPositioner {
    pub fn new() -> Self {
        // Try to detect Windows Terminal first
        if let Some(wt_hwnd) = find_windows_terminal_hwnd() {
            tracing::debug!("Detected Windows Terminal, using its window handle");
            return Self {
                hwnd: wt_hwnd,
                is_windows_terminal: true,
            };
        }

        // Fall back to console window
        let hwnd = unsafe { GetConsoleWindow() };
        tracing::debug!(
            "Using console window handle: {:?} (null={})",
            hwnd,
            hwnd.0.is_null()
        );
        Self {
            hwnd,
            is_windows_terminal: false,
        }
    }
}

impl WindowPositioner for WindowsPositioner {
    fn get_position(&self) -> Result<WindowRect> {
        if self.hwnd.0.is_null() {
            anyhow::bail!("No window handle available");
        }

        let mut rect = RECT::default();
        unsafe {
            GetWindowRect(self.hwnd, &mut rect).context("GetWindowRect failed")?;
        }

        let result = WindowRect {
            x: rect.left,
            y: rect.top,
            width: (rect.right - rect.left) as u32,
            height: (rect.bottom - rect.top) as u32,
        };

        tracing::debug!(
            "Got window position: ({}, {}) {}x{} (WT={})",
            result.x,
            result.y,
            result.width,
            result.height,
            self.is_windows_terminal
        );

        Ok(result)
    }

    fn set_position(&self, rect: &WindowRect) -> Result<()> {
        if self.hwnd.0.is_null() {
            anyhow::bail!("No window handle available");
        }

        tracing::debug!(
            "Setting window position: ({}, {}) {}x{} (WT={})",
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            self.is_windows_terminal
        );

        unsafe {
            SetWindowPos(
                self.hwnd,
                HWND_TOP,
                rect.x,
                rect.y,
                rect.width as i32,
                rect.height as i32,
                SWP_NOZORDER,
            )
            .context("SetWindowPos failed")?;
        }

        Ok(())
    }

    fn get_screen_bounds(&self) -> Result<Vec<ScreenInfo>> {
        let mut monitors: Vec<ScreenInfo> = Vec::new();

        unsafe {
            // Use a Box to pass the vector through the callback
            let monitors_ptr = &mut monitors as *mut Vec<ScreenInfo>;

            let result = EnumDisplayMonitors(
                HDC::default(),
                None,
                Some(enum_monitors_callback),
                LPARAM(monitors_ptr as isize),
            );

            if !result.as_bool() {
                anyhow::bail!("EnumDisplayMonitors failed");
            }
        }

        if monitors.is_empty() {
            // Fallback: return a default screen
            monitors.push(ScreenInfo::new(0, 0, 1920, 1080));
        }

        Ok(monitors)
    }
}

/// Find the Windows Terminal window handle by walking up the process tree.
fn find_windows_terminal_hwnd() -> Option<HWND> {
    let current_pid = unsafe { GetCurrentProcessId() };
    tracing::debug!("Current process ID: {}", current_pid);

    // Walk up the process tree looking for WindowsTerminal.exe
    let mut search_pid = current_pid;
    let mut wt_pid: Option<u32> = None;

    for _ in 0..10 {
        // Max depth to prevent infinite loops
        match get_parent_process(search_pid) {
            Some((parent_pid, parent_name)) => {
                tracing::debug!(
                    "Process {} parent: {} ({})",
                    search_pid,
                    parent_pid,
                    parent_name
                );

                if parent_name.to_lowercase() == "windowsterminal.exe" {
                    wt_pid = Some(parent_pid);
                    break;
                }
                search_pid = parent_pid;
            }
            None => break,
        }
    }

    let wt_pid = wt_pid?;
    tracing::debug!("Found Windows Terminal process: {}", wt_pid);

    // Find the main window for WindowsTerminal.exe
    find_window_for_process(wt_pid)
}

/// Get parent process ID and name for a given process.
fn get_parent_process(pid: u32) -> Option<(u32, String)> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID == pid {
                    let parent_pid = entry.th32ParentProcessID;

                    // Now find the parent's name
                    let mut search = PROCESSENTRY32W {
                        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                        ..Default::default()
                    };

                    if Process32FirstW(snapshot, &mut search).is_ok() {
                        loop {
                            if search.th32ProcessID == parent_pid {
                                let name = String::from_utf16_lossy(&search.szExeFile)
                                    .trim_end_matches('\0')
                                    .to_string();
                                let _ = windows::Win32::Foundation::CloseHandle(snapshot);
                                return Some((parent_pid, name));
                            }

                            if Process32NextW(snapshot, &mut search).is_err() {
                                break;
                            }
                        }
                    }

                    let _ = windows::Win32::Foundation::CloseHandle(snapshot);
                    return Some((parent_pid, String::new()));
                }

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = windows::Win32::Foundation::CloseHandle(snapshot);
        None
    }
}

/// Find a visible window belonging to a specific process.
fn find_window_for_process(target_pid: u32) -> Option<HWND> {
    struct EnumContext {
        target_pid: u32,
        found_hwnd: Option<HWND>,
    }

    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut EnumContext);

        // Check if window belongs to target process
        let mut window_pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut window_pid));

        if window_pid == ctx.target_pid && IsWindowVisible(hwnd).as_bool() {
            // Check if this is a main window (has reasonable size)
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_ok() {
                let width = rect.right - rect.left;
                let height = rect.bottom - rect.top;

                // Windows Terminal main window should be reasonably sized
                if width > 100 && height > 100 {
                    tracing::debug!(
                        "Found Windows Terminal window: {:?} ({}x{})",
                        hwnd,
                        width,
                        height
                    );
                    ctx.found_hwnd = Some(hwnd);
                    return BOOL::from(false); // Stop enumeration
                }
            }
        }

        BOOL::from(true) // Continue enumeration
    }

    let mut ctx = EnumContext {
        target_pid,
        found_hwnd: None,
    };

    unsafe {
        let _ = EnumWindows(Some(enum_callback), LPARAM(&mut ctx as *mut _ as isize));
    }

    ctx.found_hwnd
}

/// Callback for EnumDisplayMonitors.
unsafe extern "system" fn enum_monitors_callback(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let monitors = &mut *(lparam.0 as *mut Vec<ScreenInfo>);

    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    if GetMonitorInfoW(monitor, &mut info).as_bool() {
        let rect = info.rcMonitor;
        monitors.push(ScreenInfo::new(
            rect.left,
            rect.top,
            (rect.right - rect.left) as u32,
            (rect.bottom - rect.top) as u32,
        ));
    }

    BOOL::from(true) // Continue enumeration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_positioner_creation() {
        let positioner = WindowsPositioner::new();
        // Just verify it doesn't panic
        let _ = positioner.get_position();
    }

    #[test]
    fn test_get_parent_process() {
        let pid = unsafe { GetCurrentProcessId() };
        // Should be able to get parent (at least the shell or IDE)
        let result = get_parent_process(pid);
        // May or may not find parent depending on test environment
        println!("Parent process: {:?}", result);
    }
}
