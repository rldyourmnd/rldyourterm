use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CoreError {
    #[error("invalid grid size: {width}x{height}")]
    InvalidGridSize { width: u16, height: u16 },
    #[error("invalid grid position row={row}, col={col} for grid {width}x{height}")]
    InvalidGridPosition {
        row: u16,
        col: u16,
        width: u16,
        height: u16,
    },
}
