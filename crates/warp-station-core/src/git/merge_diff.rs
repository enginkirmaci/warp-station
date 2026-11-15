//! Base-aligned rows for the merge editor's whole-file diff.
//!
//! The two side panes show the stage files; a plain pair of lists leaves the
//! reader to find the corresponding line by eye. Here the base file is the
//! spine: every base line is a row, each side's matching or replacement line
//! sits on that row, and a side's inserted lines get rows of their own. A
//! missing cell is a gap (the line was deleted on that side), which the editor
//! paints as a hatched hole. With no base (both sides added the file) the
//! sides pair by a two-way line match.

/// One displayed row: the base line and each side's line, as absolute
/// 0-based line indexes in their own file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MergeRow {
    pub base: Option<usize>,
    pub local: Option<usize>,
    pub incoming: Option<usize>,
}

/// A file's lines without their terminators: `\n` and a preceding `\r` are
/// dropped, and a final newline does not add an empty line.
pub fn display_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') {
        lines.pop();
    }
    lines
        .into_iter()
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

/// The matching line pairs between two files, increasing on both sides.
fn line_matches(base: &[&str], side: &[&str]) -> Vec<(usize, usize)> {
    let mut prefix = 0;
    while prefix < base.len() && prefix < side.len() && base[prefix] == side[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < base.len() - prefix
        && suffix < side.len() - prefix
        && base[base.len() - 1 - suffix] == side[side.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let base_middle = &base[prefix..base.len() - suffix];
    let side_middle = &side[prefix..side.len() - suffix];
    let mut matches: Vec<(usize, usize)> = (0..prefix).map(|index| (index, index)).collect();
    matches.extend(
        longest_common(base_middle, side_middle)
            .into_iter()
            .map(|(base_index, side_index)| (base_index + prefix, side_index + prefix)),
    );
    matches.extend(
        (0..suffix).map(|offset| (base.len() - suffix + offset, side.len() - suffix + offset)),
    );
    matches
}

/// The common subsequence of lines. A table too large for memory is treated
/// as no common lines: the two sides then pair by position, which still
/// reads as a side-by-side diff.
fn longest_common(base: &[&str], side: &[&str]) -> Vec<(usize, usize)> {
    if base.is_empty() || side.is_empty() {
        return Vec::new();
    }
    let columns = side.len() + 1;
    let cells = (base.len() + 1).saturating_mul(columns);
    if cells > 4_000_000 {
        return Vec::new();
    }
    let mut table = vec![0u32; cells];
    for base_index in (0..base.len()).rev() {
        for side_index in (0..side.len()).rev() {
            table[base_index * columns + side_index] = if base[base_index] == side[side_index] {
                table[(base_index + 1) * columns + side_index + 1] + 1
            } else {
                table[(base_index + 1) * columns + side_index]
                    .max(table[base_index * columns + side_index + 1])
            };
        }
    }
    let mut pairs = Vec::new();
    let mut base_index = 0;
    let mut side_index = 0;
    while base_index < base.len() && side_index < side.len() {
        if base[base_index] == side[side_index] {
            pairs.push((base_index, side_index));
            base_index += 1;
            side_index += 1;
        } else if table[(base_index + 1) * columns + side_index]
            >= table[base_index * columns + side_index + 1]
        {
            base_index += 1;
        } else {
            side_index += 1;
        }
    }
    pairs
}

/// Pairs each side line with a base line: matches and replacements by
/// position, and a side line with no base line left in its region is an
/// insertion, anchored before the next matched base line.
fn pair(
    base_len: usize,
    side_len: usize,
    matches: &[(usize, usize)],
) -> (Vec<Option<usize>>, Vec<Vec<usize>>) {
    let mut paired = vec![None; base_len];
    let mut inserts = vec![Vec::new(); base_len + 1];
    let mut base_index = 0;
    let mut side_index = 0;
    for &(matched_base, matched_side) in matches {
        let base_run = matched_base.saturating_sub(base_index);
        let side_run = matched_side.saturating_sub(side_index);
        let shared = base_run.min(side_run);
        for offset in 0..shared {
            paired[base_index + offset] = Some(side_index + offset);
        }
        for offset in shared..side_run {
            inserts[matched_base].push(side_index + offset);
        }
        paired[matched_base] = Some(matched_side);
        base_index = matched_base + 1;
        side_index = matched_side + 1;
    }
    let base_run = base_len.saturating_sub(base_index);
    let side_run = side_len.saturating_sub(side_index);
    let shared = base_run.min(side_run);
    for offset in 0..shared {
        paired[base_index + offset] = Some(side_index + offset);
    }
    for offset in shared..side_run {
        inserts[base_len].push(side_index + offset);
    }
    (paired, inserts)
}

/// Builds the aligned rows for the three stages.
pub fn align(local: &str, base: &str, incoming: &str) -> Vec<MergeRow> {
    let local_lines = display_lines(local);
    let base_lines = display_lines(base);
    let incoming_lines = display_lines(incoming);
    let (local_of_base, local_inserts) = pair(
        base_lines.len(),
        local_lines.len(),
        &line_matches(&base_lines, &local_lines),
    );
    let (incoming_of_base, incoming_inserts) = pair(
        base_lines.len(),
        incoming_lines.len(),
        &line_matches(&base_lines, &incoming_lines),
    );
    let inserted = local_inserts
        .iter()
        .map(Vec::len)
        .chain(incoming_inserts.iter().map(Vec::len))
        .sum::<usize>();
    let mut rows = Vec::with_capacity(base_lines.len() + inserted);
    for anchor in 0..=base_lines.len() {
        let count = local_inserts[anchor]
            .len()
            .max(incoming_inserts[anchor].len());
        for offset in 0..count {
            rows.push(MergeRow {
                base: None,
                local: local_inserts[anchor].get(offset).copied(),
                incoming: incoming_inserts[anchor].get(offset).copied(),
            });
        }
        if anchor < base_lines.len() {
            rows.push(MergeRow {
                base: Some(anchor),
                local: local_of_base[anchor],
                incoming: incoming_of_base[anchor],
            });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(base: Option<usize>, local: Option<usize>, incoming: Option<usize>) -> MergeRow {
        MergeRow {
            base,
            local,
            incoming,
        }
    }

    #[test]
    fn identical_files_are_one_row_per_line() {
        let text = "a\nb\nc\n";
        let rows = align(text, text, text);
        assert_eq!(
            rows,
            vec![
                row(Some(0), Some(0), Some(0)),
                row(Some(1), Some(1), Some(1)),
                row(Some(2), Some(2), Some(2)),
            ]
        );
    }

    #[test]
    fn a_replacement_pairs_the_lines_by_position() {
        // base a,b,c; local a,B,c; incoming a,b,C
        let rows = align("a\nB\nc\n", "a\nb\nc\n", "a\nb\nC\n");
        assert_eq!(
            rows,
            vec![
                row(Some(0), Some(0), Some(0)),
                row(Some(1), Some(1), Some(1)),
                row(Some(2), Some(2), Some(2)),
            ]
        );
    }

    #[test]
    fn insertions_get_their_own_rows_and_deletions_are_gaps() {
        // local inserts X after a; incoming deletes b
        let rows = align("a\nX\nb\nc\n", "a\nb\nc\n", "a\nc\n");
        assert_eq!(
            rows,
            vec![
                row(Some(0), Some(0), Some(0)),
                row(None, Some(1), None),
                row(Some(1), Some(2), None),
                row(Some(2), Some(3), Some(1)),
            ]
        );
    }

    #[test]
    fn a_hoisted_prefix_still_aligns_on_the_base() {
        // the owner's shape: base a,b,c; local a,b,LONG; incoming b,c,Z
        let rows = align("a\nb\nLONG\n", "a\nb\nc\n", "b\nc\nZ\n");
        assert_eq!(
            rows,
            vec![
                row(Some(0), Some(0), None),
                row(Some(1), Some(1), Some(0)),
                row(Some(2), Some(2), Some(1)),
                row(None, None, Some(2)),
            ]
        );
    }

    #[test]
    fn no_base_pairs_the_two_sides() {
        let rows = align("a\nb\n", "", "a\nc\n");
        assert_eq!(
            rows,
            vec![row(None, Some(0), Some(0)), row(None, Some(1), Some(1))]
        );
    }

    #[test]
    fn display_lines_drops_the_terminators_but_keeps_empty_lines() {
        assert_eq!(display_lines("a\r\n\r\nb\n"), vec!["a", "", "b"]);
        assert_eq!(display_lines(""), Vec::<&str>::new());
        assert_eq!(display_lines("a\n"), vec!["a"]);
        assert_eq!(display_lines("a"), vec!["a"]);
    }
}
