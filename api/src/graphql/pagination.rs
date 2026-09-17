//! Relay connection helpers shared by every paginated list (ticket queues, message
//! threads, once step 5 adds them).

use anyhow::{Result, anyhow};
use async_graphql::connection::{Connection, Edge, EmptyFields};

/// Validate and extract `(page_size, is_last_mode)` from GraphQL `first`/`last` args.
///
/// Returns an error if both are specified, either is negative, or either exceeds
/// `max_size`. Returns `(default_size, false)` when neither is specified.
pub fn pagination_args(
    first: Option<i32>,
    last: Option<i32>,
    default_size: usize,
    max_size: usize,
) -> Result<(usize, bool)> {
    match (first, last) {
        (Some(_), Some(_)) => Err(anyhow!("Cannot specify both first and last")),
        (Some(v), None) => {
            if v < 0 {
                return Err(anyhow!("first must be non-negative"));
            }
            if v as usize > max_size {
                return Err(anyhow!("first must be less than or equal to {}", max_size));
            }
            Ok((v as usize, false))
        }
        (None, Some(v)) => {
            if v < 0 {
                return Err(anyhow!("last must be non-negative"));
            }
            if v as usize > max_size {
                return Err(anyhow!("last must be less than or equal to {}", max_size));
            }
            Ok((v as usize, true))
        }
        (None, None) => Ok((default_size, false)),
    }
}

/// Build a Relay connection from rows returned by the DB layer.
///
/// The DB layer is expected to have fetched `page_size + 1` rows so that
/// `hasNextPage`/`hasPreviousPage` can be inferred from the count. `to_edge` maps
/// each row into its `(cursor, node)` pair.
pub fn build_connection<T, N, F>(
    rows: Vec<T>,
    page_size: usize,
    is_last_mode: bool,
    has_after: bool,
    has_before: bool,
    to_edge: F,
) -> Connection<String, N, EmptyFields, EmptyFields>
where
    F: Fn(&T) -> (String, N),
    N: async_graphql::OutputType,
{
    let mut rows = rows;
    let fetched_extra = if rows.len() > page_size {
        rows.truncate(page_size);
        true
    } else {
        false
    };
    if is_last_mode {
        rows.reverse();
    }

    let has_previous_page = if is_last_mode {
        fetched_extra || has_after
    } else {
        has_after
    };
    let has_next_page = if is_last_mode {
        has_before
    } else {
        fetched_extra
    };

    let mut conn = Connection::new(has_previous_page, has_next_page);
    conn.edges = rows
        .iter()
        .map(|row| {
            let (cursor, node) = to_edge(row);
            Edge::new(cursor, node)
        })
        .collect();
    conn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_args_rejects_both_first_and_last() {
        assert!(pagination_args(Some(1), Some(1), 10, 50).is_err());
    }

    #[test]
    fn pagination_args_rejects_negative_first() {
        assert!(pagination_args(Some(-1), None, 10, 50).is_err());
    }

    #[test]
    fn pagination_args_rejects_negative_last() {
        assert!(pagination_args(None, Some(-1), 10, 50).is_err());
    }

    #[test]
    fn pagination_args_rejects_first_over_max() {
        assert!(pagination_args(Some(51), None, 10, 50).is_err());
    }

    #[test]
    fn pagination_args_rejects_last_over_max() {
        assert!(pagination_args(None, Some(51), 10, 50).is_err());
    }

    #[test]
    fn pagination_args_defaults_when_neither_given() {
        assert_eq!(pagination_args(None, None, 10, 50).unwrap(), (10, false));
    }

    #[test]
    fn pagination_args_uses_first() {
        assert_eq!(pagination_args(Some(5), None, 10, 50).unwrap(), (5, false));
    }

    #[test]
    fn pagination_args_uses_last() {
        assert_eq!(pagination_args(None, Some(5), 10, 50).unwrap(), (5, true));
    }

    fn edge(row: &i32) -> (String, i32) {
        (row.to_string(), *row)
    }

    #[test]
    fn build_connection_with_no_extra_row_has_no_next_page() {
        let conn = build_connection(vec![1, 2, 3], 3, false, false, false, edge);
        assert_eq!(conn.edges.len(), 3);
        assert!(!conn.has_next_page);
        assert!(!conn.has_previous_page);
    }

    #[test]
    fn build_connection_with_an_extra_row_has_a_next_page_and_truncates() {
        // DB layer fetched page_size + 1 = 4 rows to detect a next page.
        let conn = build_connection(vec![1, 2, 3, 4], 3, false, false, false, edge);
        assert_eq!(conn.edges.len(), 3);
        assert!(conn.has_next_page);
        assert!(!conn.has_previous_page);
    }

    #[test]
    fn build_connection_forward_mode_has_previous_page_when_after_was_given() {
        let conn = build_connection(vec![1, 2, 3], 3, false, true, false, edge);
        assert!(conn.has_previous_page);
        assert!(!conn.has_next_page);
    }

    #[test]
    fn build_connection_last_mode_reverses_rows_and_flags_previous_page() {
        // `last: 3` over 4 available rows: the DB fetched one extra (descending),
        // so build_connection must truncate, reverse back to natural order, and
        // report hasPreviousPage (more rows exist before this page).
        let conn = build_connection(vec![4, 3, 2, 1], 3, true, false, false, edge);
        let values: Vec<i32> = conn.edges.iter().map(|e| e.node).collect();
        assert_eq!(values, vec![2, 3, 4]);
        assert!(conn.has_previous_page);
        assert!(!conn.has_next_page);
    }

    #[test]
    fn build_connection_last_mode_has_next_page_when_before_was_given() {
        let conn = build_connection(vec![3, 2, 1], 3, true, false, true, edge);
        assert!(conn.has_next_page);
    }
}
