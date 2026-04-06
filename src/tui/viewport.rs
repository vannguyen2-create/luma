/// Per-block render cache with offset index — no flat Vec clone.
use crate::tui::block::{render_block_mut, Block};
use crate::tui::text::Line;

/// Cached render result for a single block.
struct CachedBlock {
    lines: Vec<Line>,
    is_dirty: bool,
}

/// Manages per-block caches and provides windowed access via offset index.
pub struct Viewport {
    blocks: Vec<CachedBlock>,
    /// offsets[i] = cumulative line count before block i.
    offsets: Vec<usize>,
    total: usize,
    /// Reusable buffer for visible window — avoids alloc per frame.
    window: Vec<Line>,
}

impl Viewport {
    /// Create an empty viewport.
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            offsets: Vec::new(),
            total: 0,
            window: Vec::new(),
        }
    }

    /// Add a pre-rendered block (used by tests).
    #[cfg(test)]
    pub fn push(&mut self, lines: Vec<Line>) {
        self.offsets.push(self.total);
        self.total += lines.len();
        self.blocks.push(CachedBlock {
            lines,
            is_dirty: false,
        });
    }

    /// Get text content at an absolute line index (used by tests).
    #[cfg(test)]
    pub fn text_at(&self, abs_line: usize) -> String {
        if abs_line >= self.total {
            return String::new();
        }
        let bi = self.block_at(abs_line);
        let local = abs_line - self.offsets[bi];
        self.blocks[bi]
            .lines
            .get(local)
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect())
            .unwrap_or_default()
    }

    /// Add a deferred block (not yet rendered). Marked dirty so refresh()
    /// will render it when needed. Uses estimated height of 1 line.
    pub fn push_deferred(&mut self) {
        self.offsets.push(self.total);
        self.total += 1; // estimate 1 line; corrected on first render
        self.blocks.push(CachedBlock {
            lines: vec![],
            is_dirty: true,
        });
    }

    /// Mark a single block as needing re-render.
    pub fn mark_dirty(&mut self, idx: usize) {
        if let Some(b) = self.blocks.get_mut(idx) {
            b.is_dirty = true;
        }
    }

    /// Mark all blocks as dirty (e.g. after resize).
    pub fn mark_all_dirty(&mut self) {
        for b in &mut self.blocks {
            b.is_dirty = true;
        }
    }

    /// Clear all cached data.
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.offsets.clear();
        self.total = 0;
        self.window.clear();
    }

    /// Re-render dirty blocks from source data, rebuild offsets.
    /// Only renders blocks that are visible (within scroll window) or
    /// near-visible (1 screen buffer). Deferred blocks outside the
    /// visible range stay unrendered until scrolled into view.
    pub fn refresh(
        &mut self,
        source: &mut [Block],
        width: usize,
        spinner: usize,
        scroll_offset: usize,
        view_height: usize,
    ) {
        // Determine visible block range with buffer
        let visible_end = scroll_offset + view_height * 2;
        let vis_start = if self.blocks.is_empty() {
            0
        } else {
            self.block_at(scroll_offset.min(self.total.saturating_sub(1)))
        };
        let vis_end = if self.total == 0 {
            0
        } else {
            self.block_at(visible_end.min(self.total.saturating_sub(1))) + 1
        };

        let mut any_changed = false;
        for (i, cached) in self.blocks.iter_mut().enumerate() {
            if !cached.is_dirty {
                continue;
            }
            // Always render visible blocks. Skip far-off deferred blocks.
            let in_range = i >= vis_start && i < vis_end;
            // Blocks that were previously rendered (have content) should
            // always refresh (e.g. spinner updates for pending tools).
            let was_rendered = !cached.lines.is_empty();
            if !in_range && !was_rendered {
                continue;
            }

            if let Some(block) = source.get_mut(i) {
                cached.lines = render_block_mut(block, width, spinner);
                cached.is_dirty = false;
                any_changed = true;
            }
        }
        if any_changed {
            self.rebuild_offsets();
        }
    }

    /// Total number of rendered lines across all blocks.
    pub fn total_lines(&self) -> usize {
        self.total
    }

    /// Get visible lines for the given scroll window.
    ///
    /// Fills an internal buffer from the relevant blocks — clones only
    /// the visible window (typically ~40 lines) instead of all lines.
    pub fn visible(&mut self, offset: usize, height: usize) -> &[Line] {
        self.window.clear();
        if self.blocks.is_empty() {
            return &self.window;
        }
        let end = (offset + height).min(self.total);
        if offset >= end {
            return &self.window;
        }
        let start_block = self.block_at(offset);
        let mut line_idx = offset;
        for bi in start_block..self.blocks.len() {
            if line_idx >= end {
                break;
            }
            let block_start = self.offsets[bi];
            let local_start = line_idx - block_start;
            let block = &self.blocks[bi].lines;
            for line in block.iter().skip(local_start) {
                if line_idx >= end {
                    break;
                }
                self.window.push(line.clone());
                line_idx += 1;
            }
        }
        &self.window
    }

    /// Find which block index contains the given absolute line.
    pub fn hit_test(&self, abs_line: usize) -> Option<usize> {
        if abs_line >= self.total {
            return None;
        }
        Some(self.block_at(abs_line))
    }

    /// Number of cached blocks.
    #[cfg(test)]
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Iterate lines in a window — for Renderer direct paint.
    pub fn window_iter(&self, offset: usize, height: usize) -> ViewportIter<'_> {
        let end = (offset + height).min(self.total);
        if offset >= end || self.blocks.is_empty() {
            return ViewportIter {
                blocks: &self.blocks,
                line_idx: 0,
                end: 0,
                block_idx: 0,
                local_idx: 0,
            };
        }
        let bi = self.block_at(offset);
        let local = offset - self.offsets[bi];
        ViewportIter {
            blocks: &self.blocks,
            line_idx: offset,
            end,
            block_idx: bi,
            local_idx: local,
        }
    }

    // ── Private ──

    /// Binary search for block containing abs_line.
    fn block_at(&self, abs_line: usize) -> usize {
        self.offsets
            .partition_point(|&o| o <= abs_line)
            .saturating_sub(1)
    }

    /// Rebuild offset index from block lengths.
    fn rebuild_offsets(&mut self) {
        self.offsets.clear();
        let mut acc = 0;
        for b in &self.blocks {
            self.offsets.push(acc);
            acc += b.lines.len();
        }
        self.total = acc;
    }
}

/// Zero-copy iterator over lines in a viewport window.
pub struct ViewportIter<'a> {
    blocks: &'a [CachedBlock],
    line_idx: usize,
    end: usize,
    block_idx: usize,
    local_idx: usize,
}

impl<'a> Iterator for ViewportIter<'a> {
    type Item = &'a Line;

    fn next(&mut self) -> Option<Self::Item> {
        if self.line_idx >= self.end {
            return None;
        }
        // Bounds safety: skip empty blocks, bail if out of range
        while self.block_idx < self.blocks.len()
            && self.local_idx >= self.blocks[self.block_idx].lines.len()
        {
            self.block_idx += 1;
            self.local_idx = 0;
        }
        if self.block_idx >= self.blocks.len() {
            return None;
        }
        let block = &self.blocks[self.block_idx];
        let line = &block.lines[self.local_idx];
        self.line_idx += 1;
        self.local_idx += 1;
        Some(line)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.end - self.line_idx;
        (remaining, Some(remaining))
    }
}

impl<'a> ExactSizeIterator for ViewportIter<'a> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::text::{Line, Span};
    use crate::tui::theme::palette;
    use smallvec::smallvec;

    fn dummy_lines(n: usize) -> Vec<Line> {
        (0..n).map(|_| Line::empty()).collect()
    }

    fn labeled_lines(labels: &[&str]) -> Vec<Line> {
        labels
            .iter()
            .map(|t| Line::new(smallvec![Span::new(*t, palette::FG)]))
            .collect()
    }

    #[test]
    fn push_and_total() {
        let mut vp = Viewport::new();
        vp.push(dummy_lines(3));
        vp.push(dummy_lines(5));
        assert_eq!(vp.total_lines(), 8);
    }

    #[test]
    fn visible_window() {
        let mut vp = Viewport::new();
        vp.push(dummy_lines(20));
        let vis = vp.visible(5, 10);
        assert_eq!(vis.len(), 10);
    }

    #[test]
    fn visible_clamps_at_end() {
        let mut vp = Viewport::new();
        vp.push(dummy_lines(5));
        let vis = vp.visible(3, 10);
        assert_eq!(vis.len(), 2);
    }

    #[test]
    fn hit_test_finds_block() {
        let mut vp = Viewport::new();
        vp.push(dummy_lines(3)); // block 0: lines 0-2
        vp.push(dummy_lines(5)); // block 1: lines 3-7
        vp.push(dummy_lines(2)); // block 2: lines 8-9
        assert_eq!(vp.hit_test(0), Some(0));
        assert_eq!(vp.hit_test(2), Some(0));
        assert_eq!(vp.hit_test(3), Some(1));
        assert_eq!(vp.hit_test(8), Some(2));
        assert_eq!(vp.hit_test(10), None);
    }

    #[test]
    fn clear_resets() {
        let mut vp = Viewport::new();
        vp.push(dummy_lines(5));
        vp.clear();
        assert_eq!(vp.total_lines(), 0);
        assert_eq!(vp.block_count(), 0);
    }

    #[test]
    fn mark_dirty_keeps_offsets_until_refresh() {
        let mut vp = Viewport::new();
        vp.push(dummy_lines(3));
        assert_eq!(vp.total_lines(), 3);
        vp.mark_dirty(0);
        // Offsets still valid until refresh changes line counts
        assert_eq!(vp.total_lines(), 3);
        assert_eq!(vp.block_count(), 1);
    }

    #[test]
    fn visible_across_blocks() {
        let mut vp = Viewport::new();
        vp.push(labeled_lines(&["a0", "a1", "a2"]));
        vp.push(labeled_lines(&["b0", "b1"]));
        vp.push(labeled_lines(&["c0", "c1", "c2", "c3"]));
        vp.push(labeled_lines(&["d0"]));

        let texts = |lines: &[Line]| -> Vec<String> {
            lines
                .iter()
                .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect())
                .collect()
        };

        assert_eq!(texts(vp.visible(4, 3)), vec!["b1", "c0", "c1"]);
        assert_eq!(texts(vp.visible(0, 2)), vec!["a0", "a1"]);
        assert_eq!(texts(vp.visible(9, 5)), vec!["d0"]);
        assert_eq!(texts(vp.visible(10, 5)), Vec::<String>::new());
    }

    #[test]
    fn window_iter_matches_visible() {
        let mut vp = Viewport::new();
        vp.push(labeled_lines(&["a", "b", "c"]));
        vp.push(labeled_lines(&["d", "e"]));

        let from_visible: Vec<String> = vp
            .visible(1, 3)
            .iter()
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect())
            .collect();
        let from_iter: Vec<String> = vp
            .window_iter(1, 3)
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect())
            .collect();
        assert_eq!(from_visible, from_iter);
        assert_eq!(from_iter, vec!["b", "c", "d"]);
    }

    #[test]
    fn text_at_across_blocks() {
        let mut vp = Viewport::new();
        vp.push(labeled_lines(&["hello"]));
        vp.push(labeled_lines(&["world"]));
        assert_eq!(vp.text_at(0), "hello");
        assert_eq!(vp.text_at(1), "world");
        assert_eq!(vp.text_at(2), "");
    }

    #[test]
    fn empty_viewport() {
        let mut vp = Viewport::new();
        assert_eq!(vp.total_lines(), 0);
        assert_eq!(vp.visible(0, 10).len(), 0);
        assert_eq!(vp.hit_test(0), None);
        assert_eq!(vp.window_iter(0, 10).count(), 0);
    }

    #[test]
    fn empty_block_no_panic() {
        let mut vp = Viewport::new();
        vp.push(vec![]); // empty block (e.g. empty Text stream)
        vp.push(labeled_lines(&["a"]));
        assert_eq!(vp.total_lines(), 1);
        let vis = vp.visible(0, 5);
        assert_eq!(vis.len(), 1);
        assert_eq!(vp.window_iter(0, 5).count(), 1);
    }

    #[test]
    fn multiple_empty_blocks() {
        let mut vp = Viewport::new();
        vp.push(vec![]);
        vp.push(vec![]);
        vp.push(labeled_lines(&["x"]));
        vp.push(vec![]);
        assert_eq!(vp.total_lines(), 1);
        let vis = vp.visible(0, 10);
        assert_eq!(vis.len(), 1);
    }
}
