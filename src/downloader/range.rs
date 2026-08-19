/// Split `total_size` into up to `connections` inclusive byte ranges.
pub(crate) fn calculate_byte_ranges(connections: usize, total_size: u64) -> Vec<(u64, u64)> {
    if total_size == 0 || connections == 0 {
        return Vec::new();
    }
    let connections = (connections as u64).min(total_size).max(1);
    let chunk_size = total_size.div_ceil(connections);
    (0..connections)
        .filter_map(|i| {
            let start = i * chunk_size;
            if start >= total_size {
                None
            } else {
                let end = (start + chunk_size - 1).min(total_size - 1);
                Some((start, end))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_evenly() {
        let ranges = calculate_byte_ranges(4, 100);
        assert_eq!(ranges, vec![(0, 24), (25, 49), (50, 74), (75, 99)]);
    }

    #[test]
    fn last_chunk_takes_remainder() {
        let ranges = calculate_byte_ranges(3, 10);
        assert_eq!(ranges, vec![(0, 3), (4, 7), (8, 9)]);
        assert_eq!(ranges.iter().map(|(s, e)| e - s + 1).sum::<u64>(), 10);
    }

    #[test]
    fn more_connections_than_bytes() {
        let ranges = calculate_byte_ranges(8, 3);
        assert_eq!(ranges, vec![(0, 0), (1, 1), (2, 2)]);
    }

    #[test]
    fn empty_file_has_no_ranges() {
        assert!(calculate_byte_ranges(4, 0).is_empty());
    }
}
