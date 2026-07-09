//! Parse-layer errors. Owned strings appear only on the error path.

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("line {line_no}: expected {expected} fields, found {got}")]
    ColumnCount {
        line_no: usize,
        expected: usize,
        got: usize,
    },

    #[error("line {line_no}: invalid {field}: {value:?}")]
    BadValue {
        line_no: usize,
        field: &'static str,
        value: String,
    },
}
