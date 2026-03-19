// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use regex::Regex;
use thiserror::Error;

use crate::{grid::Cell, state::TerminalState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub line_index: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub text: String,
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("search pattern must not be empty")]
    EmptyPattern,
    #[error("invalid regex pattern: {0}")]
    InvalidPattern(#[from] regex::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellSpan {
    byte_start: usize,
    byte_end: usize,
    col_start: usize,
    col_end: usize,
}

pub(crate) fn compile_search_regex(pattern: &str) -> Result<Regex, SearchError> {
    if pattern.is_empty() {
        return Err(SearchError::EmptyPattern);
    }
    Regex::new(pattern).map_err(SearchError::from)
}

pub(crate) fn search_cells(
    line_index: usize,
    cells: &[Cell],
    regex: &Regex,
    matches: &mut Vec<SearchMatch>,
) {
    let (line_text, spans) = collect_line_text_and_spans(cells);
    if line_text.is_empty() {
        return;
    }

    for regex_match in regex.find_iter(&line_text) {
        if regex_match.start() == regex_match.end() {
            continue;
        }
        let Some((start_col, end_col)) =
            byte_range_to_column_range(regex_match.start(), regex_match.end(), &spans)
        else {
            continue;
        };
        matches.push(SearchMatch {
            line_index,
            start_col,
            end_col,
            text: regex_match.as_str().to_owned(),
        });
    }
}

fn collect_line_text_and_spans(cells: &[Cell]) -> (String, Vec<CellSpan>) {
    let mut line_text = String::new();
    let mut spans = Vec::new();
    let mut display_col = 0usize;

    for cell in cells {
        if cell.width == 0 {
            continue;
        }

        let byte_start = line_text.len();
        cell.append_text_to(&mut line_text);
        let byte_end = line_text.len();
        let col_width = usize::from(cell.width.max(1));

        spans.push(CellSpan {
            byte_start,
            byte_end,
            col_start: display_col,
            col_end: display_col + col_width,
        });
        display_col += col_width;
    }

    (line_text, spans)
}

fn byte_range_to_column_range(
    byte_start: usize,
    byte_end: usize,
    spans: &[CellSpan],
) -> Option<(usize, usize)> {
    let mut start_col = None;
    let mut end_col = 0usize;

    for span in spans {
        if span.byte_end <= byte_start {
            continue;
        }
        if span.byte_start >= byte_end {
            break;
        }
        start_col.get_or_insert(span.col_start);
        end_col = span.col_end;
    }

    start_col.map(|start_col| (start_col, end_col))
}

impl TerminalState {
    pub fn search(&self, pattern: &str) -> Result<Vec<SearchMatch>, SearchError> {
        let regex = compile_search_regex(pattern)?;
        let mut matches = Vec::new();

        for (line_index, cells) in self.scrollback.iter().enumerate() {
            search_cells(line_index, cells, &regex, &mut matches);
        }

        let grid_base_line_index = self.scrollback.len();
        for row in 0..self.grid.height() {
            let cells = self
                .grid
                .row_cells(row)
                .expect("grid row in search iteration must exist");
            search_cells(
                grid_base_line_index + row as usize,
                cells,
                &regex,
                &mut matches,
            );
        }

        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::{SearchError, SearchMatch};
    use crate::{Attrs, Cell, TerminalState};

    fn cells_from_str(s: &str) -> Vec<Cell> {
        s.chars()
            .map(|ch| Cell {
                ch,
                attrs: Attrs::default(),
                width: 1,
            })
            .collect()
    }

    fn write_row(terminal: &mut TerminalState, row: u16, text: &str) {
        for (col, ch) in text.chars().enumerate() {
            terminal
                .grid
                .put_char(row, col as u16, ch, Attrs::default())
                .unwrap();
        }
    }

    #[test]
    fn search_orders_scrollback_before_live_grid() {
        let mut terminal = TerminalState::new(12, 2, 8);
        terminal
            .scrollback
            .push_from_cells(&cells_from_str("alpha beta gamma"));
        write_row(&mut terminal, 0, "beta live");

        let matches = terminal.search("beta").unwrap();
        assert_eq!(
            matches,
            vec![
                SearchMatch {
                    line_index: 0,
                    start_col: 6,
                    end_col: 10,
                    text: "beta".to_owned(),
                },
                SearchMatch {
                    line_index: 1,
                    start_col: 0,
                    end_col: 4,
                    text: "beta".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn search_reports_display_columns_after_wide_cells() {
        let mut terminal = TerminalState::new(4, 1, 0);
        terminal
            .grid
            .put_char_with_width(0, 0, '界', Attrs::default(), 2)
            .unwrap();
        terminal.grid.put_char(0, 2, 'B', Attrs::default()).unwrap();

        let matches = terminal.search("B").unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_index, 0);
        assert_eq!(matches[0].start_col, 2);
        assert_eq!(matches[0].end_col, 3);
    }

    #[test]
    fn search_rejects_empty_pattern() {
        let terminal = TerminalState::new(4, 1, 0);
        let error = terminal.search("").unwrap_err();
        assert!(matches!(error, SearchError::EmptyPattern));
    }

    #[test]
    fn search_reports_invalid_regex_pattern() {
        let terminal = TerminalState::new(4, 1, 0);
        let error = terminal.search("(").unwrap_err();
        assert!(matches!(error, SearchError::InvalidPattern(_)));
    }

    #[test]
    fn search_skips_zero_width_matches() {
        let mut terminal = TerminalState::new(4, 1, 0);
        write_row(&mut terminal, 0, "bbb");
        let matches = terminal.search("a*").unwrap();
        assert!(matches.is_empty());
    }
}
