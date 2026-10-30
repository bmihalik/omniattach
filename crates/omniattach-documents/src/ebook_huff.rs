//! Checked HUFF/CDIC decoding. No dictionary expansion cache or unbounded recursion.
use super::{append, invalid, u16_at, u32_at};
use crate::{Error, Result};

struct Phrase<'a> { bytes: &'a [u8], literal: bool }
pub(super) struct Decoder<'a> {
    fast: [(usize, bool, u64); 256],
    minimum: [u64; 33],
    maximum: [u64; 33],
    phrases: Vec<Phrase<'a>>,
}

impl<'a> Decoder<'a> {
    pub(super) fn new(records: &[&'a [u8]], budget: usize) -> Result<Self> {
        let huff = *records.first().ok_or_else(|| invalid("missing HUFF record"))?;
        if huff.get(..4) != Some(b"HUFF") || u32_at(huff, 4)? != 24 {
            return Err(invalid("invalid HUFF header"));
        }
        let fast_offset = u32_at(huff, 8)?;
        let range_offset = u32_at(huff, 12)?;
        if fast_offset < 24 || range_offset < 24
            || fast_offset.checked_add(1024).is_none_or(|n| n > huff.len())
            || range_offset.checked_add(256).is_none_or(|n| n > huff.len()) {
            return Err(invalid("HUFF tables outside record"));
        }
        let mut decoder = Self { fast: [(0, false, 0); 256], minimum: [0; 33], maximum: [0; 33], phrases: Vec::new() };
        for (index, entry) in decoder.fast.iter_mut().enumerate() {
            let value = u32_at(huff, fast_offset + index * 4)? as u64;
            let length = (value & 31) as usize;
            let terminal = value & 128 != 0;
            if length == 0 || (length <= 8 && !terminal) { return Err(invalid("invalid HUFF fast code")); }
            let maximum = (((value >> 8) + 1) << (32 - length)) - 1;
            if maximum > u32::MAX as u64 { return Err(invalid("HUFF fast code overflow")); }
            *entry = (length, terminal, maximum);
        }
        for length in 1..=32 {
            let offset = range_offset + (length - 1) * 8;
            decoder.minimum[length] = (u32_at(huff, offset)? as u64) << (32 - length);
            decoder.maximum[length] = (((u32_at(huff, offset + 4)? as u64) + 1) << (32 - length)) - 1;
            if decoder.minimum[length] > u32::MAX as u64 || decoder.maximum[length] > u32::MAX as u64 {
                return Err(invalid("HUFF range overflow"));
            }
        }
        let mut expected = None;
        for cdic in records.iter().skip(1) {
            if cdic.get(..4) != Some(b"CDIC") || u32_at(cdic, 4)? != 16 { return Err(invalid("invalid CDIC header")); }
            let total = u32_at(cdic, 8)?;
            let bits = u32_at(cdic, 12)?;
            if total == 0 || total > 1_048_576 || total > budget / std::mem::size_of::<Phrase<'a>>() {
                return Err(Error::LimitExceeded("HUFF dictionary entries".into()));
            }
            if bits > 16 || expected.is_some_and(|n| n != total) || decoder.phrases.len() >= total {
                return Err(invalid("inconsistent CDIC phrase count or bit width"));
            }
            expected = Some(total);
            let count = (1usize << bits).min(total - decoder.phrases.len());
            let table_end = 16 + count * 2;
            if table_end > cdic.len() { return Err(invalid("truncated CDIC offset table")); }
            for index in 0..count {
                let offset = 16 + u16_at(cdic, 16 + index * 2)?;
                if offset < table_end { return Err(invalid("CDIC phrase overlaps offset table")); }
                let size = u16_at(cdic, offset)?;
                let start = offset + 2;
                let end = start.checked_add(size & 0x7fff).filter(|&n| n <= cdic.len())
                    .ok_or_else(|| invalid("CDIC phrase outside record"))?;
                decoder.phrases.push(Phrase { bytes: &cdic[start..end], literal: size & 0x8000 != 0 });
            }
        }
        if expected != Some(decoder.phrases.len()) { return Err(invalid("incomplete CDIC dictionary")); }
        Ok(decoder)
    }

    pub(super) fn decode(&self, data: &[u8], out: &mut Vec<u8>, limit: usize, work: &mut usize) -> Result<()> {
        self.expand(data, out, limit, work, &mut Vec::new())
    }

    fn expand(&self, data: &[u8], out: &mut Vec<u8>, limit: usize, work: &mut usize, path: &mut Vec<usize>) -> Result<()> {
        if path.len() > 32 { return Err(Error::LimitExceeded("HUFF dictionary depth".into())); }
        let bits = data.len().checked_mul(8).ok_or_else(|| invalid("HUFF bit count overflow"))?;
        let mut position = 0;
        while position < bits {
            *work = work.checked_sub(1).ok_or_else(|| Error::LimitExceeded("HUFF decoding work".into()))?;
            let mut code = 0u64;
            for shift in 0..32 {
                let p = position + shift;
                let bit = if p < bits { (data[p / 8] >> (7 - p % 8)) & 1 } else { 0 };
                code = (code << 1) | u64::from(bit);
            }
            let (mut length, terminal, mut maximum) = self.fast[(code >> 24) as usize];
            if !terminal {
                while length <= 32 && code < self.minimum[length] { length += 1; }
                if length > 32 { return Err(invalid("invalid HUFF code length")); }
                maximum = self.maximum[length];
            }
            if length > bits - position {
                if bits - position <= 7 { break; }
                return Err(invalid("truncated HUFF code"));
            }
            let delta = maximum.checked_sub(code).ok_or_else(|| invalid("invalid HUFF code range"))?;
            let index = (delta >> (32 - length)) as usize;
            let phrase = self.phrases.get(index).ok_or_else(|| invalid("HUFF dictionary index outside table"))?;
            position += length;
            if phrase.literal {
                *work = work.checked_sub(phrase.bytes.len()).ok_or_else(|| Error::LimitExceeded("HUFF decoding work".into()))?;
                append(out, phrase.bytes, limit)?;
            } else {
                if path.contains(&index) { return Err(invalid("cyclic HUFF dictionary phrase")); }
                path.push(index);
                self.expand(phrase.bytes, out, limit, work, path)?;
                path.pop();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put32(data: &mut [u8], offset: usize, value: u32) { data[offset..offset + 4].copy_from_slice(&value.to_be_bytes()); }
    fn dictionary() -> (Vec<u8>, Vec<u8>) {
        let mut huff = vec![0; 24 + 1024 + 256];
        huff[..4].copy_from_slice(b"HUFF");
        put32(&mut huff, 4, 24); put32(&mut huff, 8, 24); put32(&mut huff, 12, 1048);
        for index in 0..256 { put32(&mut huff, 24 + index * 4, (255 << 8) | 128 | 8); }
        let mut cdic = vec![0; 16 + 512];
        cdic[..4].copy_from_slice(b"CDIC");
        put32(&mut cdic, 4, 16); put32(&mut cdic, 8, 256); put32(&mut cdic, 12, 8);
        for index in 0..256 {
            let offset = (cdic.len() - 16) as u16;
            cdic[16 + index * 2..18 + index * 2].copy_from_slice(&offset.to_be_bytes());
            cdic.extend_from_slice(&0x8001u16.to_be_bytes()); cdic.push(index as u8);
        }
        (huff, cdic)
    }
    #[test]
    fn literal_dictionary_roundtrip_and_budgets() {
        let (h, c) = dictionary();
        let decoder = Decoder::new(&[&h, &c], 65536).unwrap();
        let text = b"<p>Hello bounded HUFF</p>";
        let compressed: Vec<u8> = text.iter().map(|b| 255 - b).collect();
        let mut out = Vec::new(); let mut fuel = 10000;
        decoder.decode(&compressed, &mut out, 1000, &mut fuel).unwrap();
        assert_eq!(out, text);
        assert!(decoder.decode(&compressed, &mut Vec::new(), 2, &mut 10000).is_err());
        assert!(decoder.decode(&compressed, &mut Vec::new(), 1000, &mut 0).is_err());
    }
    #[test]
    fn recursive_phrases_and_cycles() {
        let (h, mut c) = dictionary();
        c[528..530].copy_from_slice(&1u16.to_be_bytes()); c[530] = 254;
        let decoder = Decoder::new(&[&h, &c], 65536).unwrap();
        let mut out = Vec::new();
        decoder.decode(&[255], &mut out, 100, &mut 1000).unwrap();
        assert_eq!(out, [1]);
        c[530] = 255;
        let decoder = Decoder::new(&[&h, &c], 65536).unwrap();
        assert!(decoder.decode(&[255], &mut Vec::new(), 100, &mut 1000).is_err());
    }
    #[test]
    fn rejects_truncated_or_invalid_tables() {
        let (mut h, c) = dictionary();
        assert!(Decoder::new(&[&h, &c[..15]], 65536).is_err());
        put32(&mut h, 8, u32::MAX);
        assert!(Decoder::new(&[&h, &c], 65536).is_err());
    }
    #[cfg(feature = "azw3")]
    #[test]
    fn generated_azw3_with_huff_and_both_trailer_flags() {
        use crate::{Options, extract_bytes};
        for flags in [1u16, 3] {
            let (h, c) = dictionary();
            let html = b"<html><body><p>Hello HUFF book</p></body></html>";
            let mut text: Vec<u8> = html.iter().map(|b| 255 - b).collect();
            text.push(0);
            if flags == 3 { text.push(0x81); }
            let mut header = vec![0; 280];
            header[0..2].copy_from_slice(&17480u16.to_be_bytes());
            put32(&mut header, 4, html.len() as u32);
            header[8..10].copy_from_slice(&1u16.to_be_bytes());
            header[16..20].copy_from_slice(b"MOBI");
            put32(&mut header, 20, 264); put32(&mut header, 28, 65001); put32(&mut header, 36, 8);
            put32(&mut header, 112, 2); put32(&mut header, 116, 2);
            header[242..244].copy_from_slice(&flags.to_be_bytes());
            let records = [header, text, h, c];
            let mut bytes = vec![0; 112];
            bytes[60..68].copy_from_slice(b"BOOKMOBI"); bytes[76..78].copy_from_slice(&4u16.to_be_bytes());
            for (index, record) in records.iter().enumerate() {
                let offset = bytes.len() as u32; put32(&mut bytes, 78 + index * 8, offset); bytes.extend_from_slice(record);
            }
            let doc = extract_bytes("generated.azw3", &bytes, &Options::default()).unwrap();
            assert!(doc.to_text().contains("Hello HUFF book"));
            assert!(doc.to_markdown().contains("Hello HUFF book"));
            assert_eq!(doc.source.byte_length, bytes.len());
            let mut encrypted = bytes.clone(); encrypted[112 + 12..112 + 14].copy_from_slice(&1u16.to_be_bytes());
            assert!(extract_bytes("encrypted.azw3", &encrypted, &Options::default()).is_err());
        }
    }
}
