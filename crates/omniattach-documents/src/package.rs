use crate::{Error, Options, Result};
use std::{
    collections::HashSet,
    io::{Cursor, Read},
};

pub(crate) struct Package<'a> {
    zip: zip::ZipArchive<Cursor<&'a [u8]>>,
    limit: u64,
}
impl<'a> Package<'a> {
    pub fn open(bytes: &'a [u8], options: &Options) -> Result<Self> {
        let mut zip =
            zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| Error::Parse(e.to_string()))?;
        if zip.len() > options.limits.archive_entries {
            return Err(Error::LimitExceeded("archive entries".into()));
        }
        let mut total = 0u64;
        let mut names = HashSet::new();
        for i in 0..zip.len() {
            let file = zip.by_index(i).map_err(|e| Error::Parse(e.to_string()))?;
            let name = file.name().map_err(|e| Error::Parse(e.to_string()))?;
            if file.enclosed_name().is_none()
                || name.contains('\\')
                || !names.insert(name.into_owned())
            {
                return Err(Error::InvalidInput(
                    "unsafe or duplicate archive member".into(),
                ));
            }
            total = total
                .checked_add(file.size())
                .ok_or_else(|| Error::LimitExceeded("archive size overflow".into()))?;
            if total > options.limits.archive_bytes {
                return Err(Error::LimitExceeded("uncompressed archive bytes".into()));
            }
        }
        Ok(Self {
            zip,
            limit: options.limits.archive_bytes,
        })
    }
    pub fn contains(&self, name: &str) -> bool {
        self.zip.index_for_name(name).is_some()
    }
    pub fn member_names(&self) -> Vec<String> {
        self.zip
            .file_names()
            .filter_map(|n| n.ok().map(|s| s.into_owned()))
            .collect()
    }
    pub fn text(&mut self, name: &str) -> Result<String> {
        let data = self.bytes(name)?;
        super::text::decode(&data, None).map(|x| x.0)
    }
    pub fn bytes(&mut self, name: &str) -> Result<Vec<u8>> {
        let file = self
            .zip
            .by_name(name)
            .map_err(|e| Error::Parse(format!("{name}: {e}")))?;
        let mut data = Vec::new();
        file.take(self.limit + 1).read_to_end(&mut data)?;
        if data.len() as u64 > self.limit {
            return Err(Error::LimitExceeded("archive member bytes".into()));
        }
        Ok(data)
    }
}
/// Resolve an internal package relationship and reject schemes, absolute paths and root escape.
pub(crate) fn resolve(base: &str, href: &str) -> Result<String> {
    let href = href.split('#').next().unwrap_or("");
    let href = percent_encoding::percent_decode_str(href)
        .decode_utf8()
        .map_err(|e| Error::InvalidInput(e.to_string()))?;
    if href.starts_with('/') || href.contains([':', '\\', '?', '\0']) {
        return Err(Error::InvalidInput(format!(
            "external/unsafe package reference: {href}"
        )));
    }
    let mut parts: Vec<&str> = base
        .rsplit_once('/')
        .map(|(p, _)| p.split('/').collect())
        .unwrap_or_default();
    for p in href.split('/') {
        match p {
            "." | "" => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(Error::InvalidInput("package reference escapes root".into()));
                }
            }
            _ => parts.push(p),
        }
    }
    Ok(parts.join("/"))
}
