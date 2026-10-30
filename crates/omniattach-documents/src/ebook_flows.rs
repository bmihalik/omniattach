//! Validate a referenced KF8 FDST table and select its primary flow, without copying.
use super::{invalid, u32_at};
use crate::{Error, Result};

pub(super) fn first_flow<'a>(
    decoded: &'a [u8],
    declared: usize,
    fdst: &[u8],
    header_count: usize,
    max_flows: usize,
) -> Result<(&'a [u8], usize)> {
    if fdst.get(..4) != Some(b"FDST") {
        return Err(invalid("referenced FDST record has invalid signature"));
    }
    let start = u32_at(fdst, 4)?;
    let count = u32_at(fdst, 8)?;
    if count == 0 || count != header_count {
        return Err(invalid("FDST flow count differs from KF8 header"));
    }
    if count > max_flows {
        return Err(Error::LimitExceeded("KF8 flows".into()));
    }
    let end = count
        .checked_mul(8)
        .and_then(|n| start.checked_add(n))
        .filter(|&n| n <= fdst.len())
        .ok_or_else(|| invalid("FDST table exceeds record bounds"))?;
    if start < 12 || end < start {
        return Err(invalid("invalid FDST table offset"));
    }
    let mut previous = 0;
    let mut primary_end = 0;
    for index in 0..count {
        let offset = start + index * 8;
        let first = u32_at(fdst, offset)?;
        let last = u32_at(fdst, offset + 4)?;
        if first != previous || last < first || last > decoded.len() {
            return Err(invalid(
                "FDST ranges are not contiguous ordered in-bounds flows",
            ));
        }
        if index == 0 {
            primary_end = last;
        }
        previous = last;
    }
    if previous != decoded.len() {
        return Err(invalid(&format!(
            "FDST final endpoint {previous} differs from decoded length {}",
            decoded.len()
        )));
    }
    if primary_end != declared {
        return Err(invalid(&format!(
            "FDST primary endpoint {primary_end} differs from declared text length {declared}"
        )));
    }
    Ok((&decoded[..primary_end], count - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn table(ranges: &[(usize, usize)]) -> Vec<u8> {
        let mut bytes = b"FDST".to_vec();
        bytes.extend_from_slice(&12u32.to_be_bytes());
        bytes.extend_from_slice(&(ranges.len() as u32).to_be_bytes());
        for &(start, end) in ranges {
            bytes.extend_from_slice(&(start as u32).to_be_bytes());
            bytes.extend_from_slice(&(end as u32).to_be_bytes());
        }
        bytes
    }
    #[test]
    fn reported_primary_and_total_length_patterns() {
        for (primary, total) in [(1_062_524, 1_096_756), (80_305, 80_430)] {
            let data = vec![b'x'; total];
            let fdst = table(&[(0, primary), (primary, total)]);
            let (text, skipped) = first_flow(&data, primary, &fdst, 2, 100).unwrap();
            assert_eq!(text.len(), primary);
            assert_eq!(skipped, 1);
        }
    }
    #[test]
    fn invalid_ranges_counts_and_totals_fail() {
        let data = [0; 10];
        for ranges in [
            vec![(1, 5), (5, 10)],
            vec![(0, 5), (4, 10)],
            vec![(0, 5), (6, 10)],
            vec![(0, 5), (5, 11)],
            vec![(0, 5), (5, 9)],
        ] {
            assert!(first_flow(&data, 5, &table(&ranges), 2, 100).is_err());
        }
        let fdst = table(&[(0, 5), (5, 10)]);
        assert!(first_flow(&data, 4, &fdst, 2, 100).is_err());
        assert!(first_flow(&data, 5, &fdst, 3, 100).is_err());
        assert!(first_flow(&data, 5, &fdst, 2, 1).is_err());
        assert!(first_flow(&data, 5, &fdst[..15], 2, 100).is_err());
    }
    #[cfg(feature = "azw3")]
    fn fixture(fdst_index: u32, flow_count: u32, declared: usize) -> Vec<u8> {
        let primary = b"<p>Primary book text</p>";
        let secondary = b"<p>SECONDARY_MUST_NOT_APPEAR</p>";
        let mut content = primary.to_vec();
        content.extend_from_slice(secondary);
        let mut header = vec![0; 280];
        header[..2].copy_from_slice(&1u16.to_be_bytes());
        header[4..8].copy_from_slice(&(declared as u32).to_be_bytes());
        header[8..10].copy_from_slice(&1u16.to_be_bytes());
        header[16..20].copy_from_slice(b"MOBI");
        for (offset, value) in [
            (20, 264u32),
            (28, 65001),
            (36, 8),
            (192, fdst_index),
            (196, flow_count),
        ] {
            header[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        }
        let fdst = table(&[(0, primary.len()), (primary.len(), content.len())]);
        let mut bytes = vec![0; 104];
        bytes[60..68].copy_from_slice(b"BOOKMOBI");
        bytes[76..78].copy_from_slice(&3u16.to_be_bytes());
        for (index, record) in [header, content, fdst].iter().enumerate() {
            let offset = bytes.len() as u32;
            bytes[78 + index * 8..82 + index * 8].copy_from_slice(&offset.to_be_bytes());
            bytes.extend_from_slice(record);
        }
        bytes
    }
    #[cfg(feature = "azw3")]
    #[test]
    fn library_projects_only_primary_and_rejects_invalid_reference() {
        use crate::{Options, extract_bytes};
        let declared = b"<p>Primary book text</p>".len();
        let doc =
            extract_bytes("flows.azw3", &fixture(2, 2, declared), &Options::default()).unwrap();
        assert!(doc.to_text().contains("Primary book text"));
        assert!(!doc.to_text().contains("SECONDARY_MUST_NOT_APPEAR"));
        assert!(!doc.to_markdown().contains("SECONDARY_MUST_NOT_APPEAR"));
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.code == "AZW3_SECONDARY_FLOWS_SKIPPED")
        );
        assert!(extract_bytes("bad.azw3", &fixture(99, 2, declared), &Options::default()).is_err());
        // A stray FDST record must not override an absent header reference.
        assert!(
            extract_bytes(
                "no-reference.azw3",
                &fixture(u32::MAX, 0, declared),
                &Options::default()
            )
            .is_err()
        );
    }
}
