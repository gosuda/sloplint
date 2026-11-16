/// Sorted, disjoint inclusive line ranges affected by parse rescue.
#[derive(Default)]
pub struct TaintedLines {
    ranges: Vec<(u32, u32)>,
}

impl TaintedLines {
    pub(crate) fn new(mut ranges: Vec<(u32, u32)>) -> Self {
        ranges.sort_unstable();
        ranges.dedup_by(|next, previous| {
            if next.0 <= previous.1.saturating_add(1) {
                previous.1 = previous.1.max(next.1);
                true
            } else {
                false
            }
        });
        Self { ranges }
    }

    #[must_use]
    #[inline]
    pub fn contains(&self, line: u32) -> bool {
        let at = self.ranges.partition_point(|&(_, last)| last < line);
        self.ranges.get(at).is_some_and(|&(first, _)| first <= line)
    }
}

#[cfg(test)]
mod tests {
    use super::TaintedLines;

    #[test]
    fn empty_ranges_leave_every_line_clean() {
        let ranges = TaintedLines::default();
        for line in [0, 1, u32::MAX] {
            assert!(!ranges.contains(line));
        }
    }

    #[test]
    fn overlapping_ranges_keep_inclusive_edges_and_gaps() {
        let ranges = TaintedLines::new(vec![
            (30, 30),
            (4, 8),
            (3, 5),
            (10, 12),
            (3, 5),
            (20, 25),
            (21, 22),
            (9, 9),
        ]);
        for line in [3, 4, 5, 8, 9, 10, 12, 20, 21, 22, 25, 30] {
            assert!(ranges.contains(line), "tainted line {line}");
        }
        for line in [0, 1, 2, 13, 19, 26, 29, 31, u32::MAX] {
            assert!(!ranges.contains(line), "clean line {line}");
        }
    }

    #[test]
    fn maximum_line_does_not_overflow_during_coalescing() {
        let ranges = TaintedLines::new(vec![(u32::MAX, u32::MAX), (1, u32::MAX)]);
        assert!(!ranges.contains(0));
        assert!(ranges.contains(1));
        assert!(ranges.contains(u32::MAX));
    }

    #[test]
    fn many_disjoint_ranges_keep_interleaved_lines_clean() {
        let ranges = TaintedLines::new(
            (0..51_150)
                .rev()
                .map(|index| (index * 2 + 1, index * 2 + 1))
                .collect(),
        );
        for line in [1, 50_001, 102_299] {
            assert!(ranges.contains(line));
        }
        for line in [0, 2, 50_000, 102_298, 102_300] {
            assert!(!ranges.contains(line));
        }
    }
}
