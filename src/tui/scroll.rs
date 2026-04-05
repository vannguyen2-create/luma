//! Viewport scroll state — pure arithmetic, no knowledge of content.

/// Tracks scroll offset and whether the user manually scrolled.
#[derive(Debug)]
pub struct ScrollView {
    pub offset: usize,
    pub is_user_scrolled: bool,
}

impl ScrollView {
    /// Create at top position.
    pub fn new() -> Self {
        Self {
            offset: 0,
            is_user_scrolled: false,
        }
    }

    /// Scroll up by `n` lines. Marks as user-scrolled.
    pub fn up(&mut self, n: usize) {
        self.offset = self.offset.saturating_sub(n);
        self.is_user_scrolled = true;
    }

    /// Scroll down by `n` lines within bounds. Clears user-scrolled if at bottom.
    pub fn down(&mut self, n: usize, max_scroll: usize) {
        self.offset = (self.offset + n).min(max_scroll);
        if self.offset >= max_scroll {
            self.is_user_scrolled = false;
        }
    }

    /// Jump to a specific offset. Sets user-scrolled if not at bottom.
    pub fn set_offset(&mut self, target: usize, max_scroll: usize) {
        self.offset = target.min(max_scroll);
        self.is_user_scrolled = self.offset < max_scroll;
    }

    /// Auto-scroll to bottom if user hasn't manually scrolled.
    pub fn auto_scroll(&mut self, total_lines: usize, view_height: usize) {
        if self.is_user_scrolled {
            return;
        }
        let overflow = total_lines.saturating_sub(view_height);
        if overflow > 0 {
            self.offset = overflow;
        }
    }

    /// Clamp offset after content shrinks. Clears user-scrolled if at bottom.
    pub fn clamp(&mut self, total_lines: usize, view_height: usize) {
        let max = total_lines.saturating_sub(view_height);
        if self.offset > max {
            self.offset = max;
        }
        if total_lines <= view_height || self.offset >= max {
            self.is_user_scrolled = false;
        }
    }

    /// Reset to initial state.
    pub fn reset(&mut self) {
        self.offset = 0;
        self.is_user_scrolled = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_at_zero() {
        let s = ScrollView::new();
        assert_eq!(s.offset, 0);
        assert!(!s.is_user_scrolled);
    }

    #[test]
    fn up_marks_user_scrolled() {
        let mut s = ScrollView::new();
        s.offset = 5;
        s.up(3);
        assert_eq!(s.offset, 2);
        assert!(s.is_user_scrolled);
    }

    #[test]
    fn up_clamps_to_zero() {
        let mut s = ScrollView::new();
        s.offset = 2;
        s.up(10);
        assert_eq!(s.offset, 0);
    }

    #[test]
    fn down_clears_user_scrolled_at_bottom() {
        let mut s = ScrollView::new();
        s.is_user_scrolled = true;
        s.down(100, 10);
        assert_eq!(s.offset, 10);
        assert!(!s.is_user_scrolled);
    }

    #[test]
    fn auto_scroll_respects_user_scrolled() {
        let mut s = ScrollView::new();
        s.is_user_scrolled = true;
        s.auto_scroll(100, 20);
        assert_eq!(s.offset, 0); // didn't move
    }

    #[test]
    fn auto_scroll_follows_content() {
        let mut s = ScrollView::new();
        s.auto_scroll(100, 20);
        assert_eq!(s.offset, 80);
    }

    #[test]
    fn clamp_shrinks_offset() {
        let mut s = ScrollView::new();
        s.offset = 50;
        s.clamp(30, 20); // max = 10
        assert_eq!(s.offset, 10);
    }

    #[test]
    fn set_offset_sets_user_scrolled() {
        let mut s = ScrollView::new();
        s.set_offset(5, 10);
        assert!(s.is_user_scrolled);
        s.set_offset(10, 10);
        assert!(!s.is_user_scrolled); // at bottom
    }

    #[test]
    fn reset_clears_all() {
        let mut s = ScrollView::new();
        s.offset = 42;
        s.is_user_scrolled = true;
        s.reset();
        assert_eq!(s.offset, 0);
        assert!(!s.is_user_scrolled);
    }
}
