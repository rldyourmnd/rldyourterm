#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub row: u16,
    pub col: u16,
    pub visible: bool,
}

impl Default for Cursor {
    fn default() -> Self {
        Self::new()
    }
}

impl Cursor {
    pub const fn new() -> Self {
        Self {
            row: 0,
            col: 0,
            visible: true,
        }
    }

    pub fn move_to(&mut self, row: u16, col: u16, width: u16, height: u16) -> bool {
        let next_row = clamp(row, height);
        let next_col = clamp(col, width);
        let changed = self.row != next_row || self.col != next_col;
        self.row = next_row;
        self.col = next_col;
        changed
    }

    pub fn move_relative(
        &mut self,
        row_delta: i32,
        col_delta: i32,
        width: u16,
        height: u16,
    ) -> bool {
        let raw_row = self.row as i32 + row_delta;
        let raw_col = self.col as i32 + col_delta;
        let next_row = if raw_row < 0 {
            0
        } else {
            raw_row.min(u16::MAX as i32) as u16
        };
        let next_col = if raw_col < 0 {
            0
        } else {
            raw_col.min(u16::MAX as i32) as u16
        };
        self.move_to(next_row, next_col, width, height)
    }

    pub fn carriage_return(&mut self) -> bool {
        if self.col == 0 {
            return false;
        }
        self.col = 0;
        true
    }
}

fn clamp(value: u16, max_exclusive: u16) -> u16 {
    if max_exclusive == 0 {
        0
    } else {
        value.min(max_exclusive - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::Cursor;

    #[test]
    fn move_to_clamps_to_bounds() {
        let mut cursor = Cursor::new();
        assert!(cursor.move_to(10, 10, 5, 3));
        assert_eq!(cursor.row, 2);
        assert_eq!(cursor.col, 4);
    }

    #[test]
    fn move_relative_saturates_low_end() {
        let mut cursor = Cursor::new();
        cursor.move_to(2, 2, 10, 10);
        assert!(cursor.move_relative(-10, -10, 10, 10));
        assert_eq!(cursor.row, 0);
        assert_eq!(cursor.col, 0);
    }

    #[test]
    fn carriage_return_only_changes_column() {
        let mut cursor = Cursor::new();
        cursor.move_to(3, 4, 10, 10);
        assert!(cursor.carriage_return());
        assert_eq!(cursor.row, 3);
        assert_eq!(cursor.col, 0);
        assert!(!cursor.carriage_return());
    }
}
