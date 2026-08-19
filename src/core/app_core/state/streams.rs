//! Client-generated stream delivery: routing styled lines to stream-
//! subscribed windows/tabs and page-semantics clearing for app-like
//! streams (bestiary).

use super::*;

impl AppCore {
    /// Clear every window (or tab) subscribed to a stream. Page semantics
    /// for app-like streams (bestiary): each navigation step replaces the
    /// view instead of appending to a scrollback.
    pub fn clear_stream_windows(&mut self, stream: &str) {
        use crate::data::WindowContent;
        for window in self.ui_state.windows.values_mut() {
            match &mut window.content {
                WindowContent::Text(content)
                    if content
                        .streams
                        .iter()
                        .any(|s| s.eq_ignore_ascii_case(stream)) =>
                {
                    content.lines.clear();
                    content.generation = content.generation.wrapping_add(1);
                }
                WindowContent::TabbedText(content) => {
                    for tab in content.tabs.iter_mut() {
                        if tab
                            .definition
                            .streams
                            .iter()
                            .any(|s| s.eq_ignore_ascii_case(stream))
                        {
                            tab.content.lines.clear();
                            tab.content.generation = tab.content.generation.wrapping_add(1);
                        }
                    }
                }
                _ => {}
            }
        }
        self.needs_render = true;
    }

    /// Deliver client-generated styled lines to whatever window subscribes
    /// to the given stream, falling back to the main window so the output
    /// is never silently lost (ebestiary-style scroll output by default;
    /// a dedicated custom window upgrades it).
    pub fn add_client_lines_to_stream(
        &mut self,
        stream: &str,
        lines: Vec<crate::data::StyledLine>,
    ) {
        use crate::data::WindowContent;
        for line in lines {
            if let Some(remote) = self.message_processor.remote.as_mut() {
                remote.push_text(stream, std::sync::Arc::new(line.clone()));
            }
            let mut delivered = false;
            for window in self.ui_state.windows.values_mut() {
                match &mut window.content {
                    WindowContent::Text(content)
                        if content
                            .streams
                            .iter()
                            .any(|s| s.eq_ignore_ascii_case(stream)) =>
                    {
                        content.add_line(line.clone());
                        delivered = true;
                    }
                    WindowContent::TabbedText(content) => {
                        for tab in content.tabs.iter_mut() {
                            if tab
                                .definition
                                .streams
                                .iter()
                                .any(|s| s.eq_ignore_ascii_case(stream))
                            {
                                tab.content.add_line(line.clone());
                                delivered = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            if !delivered {
                // Fall back to wherever "main" text lands — including a
                // tabbed main window (GUI default layouts), which a plain
                // named lookup misses.
                if let Some(main) = self.ui_state.get_window_mut("main") {
                    if let WindowContent::Text(content) = &mut main.content {
                        content.add_line(line);
                        continue;
                    }
                }
                'fallback: for window in self.ui_state.windows.values_mut() {
                    match &mut window.content {
                        WindowContent::Text(content)
                            if content
                                .streams
                                .iter()
                                .any(|s| s.eq_ignore_ascii_case("main")) =>
                        {
                            content.add_line(line.clone());
                            break 'fallback;
                        }
                        WindowContent::TabbedText(content) => {
                            for tab in content.tabs.iter_mut() {
                                if tab
                                    .definition
                                    .streams
                                    .iter()
                                    .any(|s| s.eq_ignore_ascii_case("main"))
                                {
                                    tab.content.add_line(line.clone());
                                    break 'fallback;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        self.needs_render = true;
    }
}
