use std::ops::Range;

/// The identity prefix of every key an indexer instance writes:
/// `<dataplane_id: u8><instance_id: u8>`. Namespacing by this prefix is what
/// lets many instances share one TiKV cluster without touching each other's
/// keyspace, and makes retiring an instance a single contiguous range delete.
#[derive(Debug, Clone, PartialEq)]
pub struct Namespace {
    pub dataplane_id: u8,
    pub instance_id: u8,
}

impl Namespace {
    pub fn new(dataplane_id: u8, instance_id: u8) -> Self {
        Self {
            dataplane_id,
            instance_id,
        }
    }

    /// Range covering every key of every instance in this dataplane.
    pub fn dataplane_key_range(&self) -> Range<Vec<u8>> {
        prefix_key_range(&[self.dataplane_id])
    }

    /// Range covering every key this instance owns.
    pub fn instance_key_range(&self) -> Range<Vec<u8>> {
        prefix_key_range(&self.encode())
    }

    pub fn encode(&self) -> Vec<u8> {
        vec![self.dataplane_id, self.instance_id]
    }

    // dataplane (u8) + instance (u8)
    pub fn size() -> usize {
        2 * size_of::<u8>()
    }
}

/// The exclusive end bound for a scan over everything under `prefix`: the
/// prefix with its last non-0xFF byte incremented (trailing 0xFF bytes
/// truncated). An all-0xFF prefix yields an unbounded (empty) end key.
pub fn prefix_key_range(prefix: &[u8]) -> Range<Vec<u8>> {
    let start = prefix.to_vec();
    let mut end = prefix.to_vec();

    for i in (0..end.len()).rev() {
        if end[i] != 255 {
            end[i] += 1;
            end.truncate(i + 1);
            return start..end;
        }
    }

    start..vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_key_range() {
        let range = Namespace::new(1, 2).instance_key_range();
        assert_eq!(range.start, vec![1, 2]);
        assert_eq!(range.end, vec![1, 3]);
    }

    #[test]
    fn test_dataplane_key_range() {
        let range = Namespace::new(1, 0).dataplane_key_range();
        assert_eq!(range.start, vec![1]);
        assert_eq!(range.end, vec![2]);
    }

    #[test]
    fn test_prefix_key_range_handles_trailing_ff() {
        let range = prefix_key_range(&[5, 255]);
        assert_eq!(range.start, vec![5, 255]);
        assert_eq!(range.end, vec![6]);

        let range = prefix_key_range(&[255, 255]);
        assert!(range.end.is_empty());
    }
}
