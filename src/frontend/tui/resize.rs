/// Debouncer for terminal resize events to prevent excessive layout recalculations
pub struct ResizeDebouncer {
    pub(crate) last_resize_time: Option<std::time::Instant>,
    pub(crate) debounce_duration: std::time::Duration,
    pub(crate) pending_size: Option<(u16, u16)>, // (width, height)
}

impl ResizeDebouncer {
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            last_resize_time: None,
            debounce_duration: std::time::Duration::from_millis(debounce_ms),
            pending_size: None,
        }
    }

    /// Check if a resize event should be processed or debounced.
    ///
    /// Returns `Some((width, height))` if the resize should be processed immediately:
    /// - Always returns Some() for the first resize
    /// - Returns Some() if debounce_duration has elapsed since the last processed resize
    /// - Returns None() if the resize is within the debounce window (and stores as pending)
    ///
    /// When None is returned, the resize dimensions are stored as pending and will be
    /// checked on the next call to `check_pending()`.
    pub fn check_resize(&mut self, width: u16, height: u16) -> Option<(u16, u16)> {
        let now = std::time::Instant::now();

        // First resize is always processed immediately
        if self.last_resize_time.is_none() {
            self.last_resize_time = Some(now);
            self.pending_size = None;
            return Some((width, height));
        }

        let last_time = self.last_resize_time.unwrap();
        let elapsed = now.duration_since(last_time);

        if elapsed >= self.debounce_duration {
            // Debounce window has passed - process this resize immediately
            self.last_resize_time = Some(now);
            self.pending_size = None;
            Some((width, height))
        } else {
            // Still within debounce window - store as pending for later
            self.pending_size = Some((width, height));
            None
        }
    }

    /// Check if there's a pending resize that should be processed.
    ///
    /// Returns `Some((width, height))` if a pending resize exists and the debounce period
    /// has elapsed since the last processed resize. Returns `None()` otherwise.
    ///
    /// This should be called on every event loop iteration to ensure pending resizes are
    /// eventually processed even if no new resize events arrive.
    pub fn check_pending(&mut self) -> Option<(u16, u16)> {
        let now = std::time::Instant::now();

        // If no resize has been processed yet, there's nothing pending
        let last_time = self.last_resize_time?;

        let elapsed = now.duration_since(last_time);

        if elapsed >= self.debounce_duration {
            if let Some(size) = self.pending_size.take() {
                self.last_resize_time = Some(now);
                return Some(size);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_resize_processed_immediately() {
        let mut debouncer = ResizeDebouncer::new(100);
        let result = debouncer.check_resize(80, 24);

        assert_eq!(result, Some((80, 24)), "First resize should be processed immediately");
    }

    #[test]
    fn test_rapid_resizes_debounced() {
        let mut debouncer = ResizeDebouncer::new(100);

        // First resize is always processed
        let result1 = debouncer.check_resize(80, 24);
        assert_eq!(result1, Some((80, 24)));

        // Rapid resizes within 100ms should be debounced
        let result2 = debouncer.check_resize(81, 24);
        assert_eq!(result2, None, "Rapid resize should be debounced");

        let result3 = debouncer.check_resize(82, 24);
        assert_eq!(result3, None, "Rapid resize should be debounced");
    }

    #[test]
    fn test_pending_resize_stored() {
        let mut debouncer = ResizeDebouncer::new(100);

        debouncer.check_resize(80, 24);
        debouncer.check_resize(90, 30);

        // The second resize should be stored as pending with latest dimensions
        assert_eq!(debouncer.pending_size, Some((90, 30)));
    }

    #[test]
    fn test_multiple_pending_resizes_store_latest() {
        let mut debouncer = ResizeDebouncer::new(100);

        debouncer.check_resize(80, 24);
        debouncer.check_resize(90, 25);
        debouncer.check_resize(100, 26);
        debouncer.check_resize(110, 27);

        // Only the latest size should be stored
        assert_eq!(debouncer.pending_size, Some((110, 27)));
    }

    #[test]
    fn test_no_pending_resize_returns_none() {
        let mut debouncer = ResizeDebouncer::new(100);

        debouncer.check_resize(80, 24);

        // Immediately calling check_pending should return None (not enough time elapsed)
        let result = debouncer.check_pending();
        assert_eq!(result, None, "check_pending should return None when debounce period not elapsed");
    }

    #[test]
    fn test_pending_resize_processed_after_debounce() {
        let mut debouncer = ResizeDebouncer::new(10); // Use 10ms for faster test

        debouncer.check_resize(80, 24);
        debouncer.check_resize(90, 30);

        // Wait for debounce period to elapse
        std::thread::sleep(std::time::Duration::from_millis(15));

        let result = debouncer.check_pending();
        assert_eq!(result, Some((90, 30)), "Pending resize should be processed after debounce period");

        // After processing, pending should be cleared
        assert_eq!(debouncer.pending_size, None);
    }

    #[test]
    fn test_resize_after_debounce_period_immediate() {
        let mut debouncer = ResizeDebouncer::new(10);

        debouncer.check_resize(80, 24);
        debouncer.check_resize(90, 30);

        // Wait for debounce period to elapse
        std::thread::sleep(std::time::Duration::from_millis(15));

        // New resize should be processed immediately
        let result = debouncer.check_resize(100, 35);
        assert_eq!(result, Some((100, 35)), "Resize after debounce period should be processed immediately");
    }
}
