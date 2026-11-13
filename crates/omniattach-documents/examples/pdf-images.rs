//! Explicit image projection API plus a new-directory example writer.
#[cfg(not(feature = "pdf"))]
fn main() {
    eprintln!("pdf-images requires --features pdf");
    std::process::exit(2);
}
#[cfg(feature = "pdf")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{io::Read, path::PathBuf};
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: pdf-images INPUT.pdf NEW_EXPORT_DIRECTORY".into());
    }
    let input = PathBuf::from(&args[0]);
    let options = omniattach_documents::Options::default();
    let file = std::fs::File::open(&input)?;
    if !file.metadata()?.is_file() {
        return Err("input is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(options.limits.input_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    let doc =
        omniattach_documents::extract_pdf_with_images(&input.to_string_lossy(), &bytes, &options)?;
    export::write_new(&doc, &PathBuf::from(&args[1]))?;
    for diagnostic in &doc.diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
    Ok(())
}
#[cfg(feature = "pdf")]
mod export {
    use omniattach_documents::{Document, Error, Result};
    use std::{collections::BTreeSet, fs, io::Write, path::Path};
    pub(super) fn write_new(doc: &Document, destination: &Path) -> Result<()> {
        let mut names = BTreeSet::new();
        for asset in &doc.assets {
            let Some(leaf) = asset.filename.strip_prefix("images/") else {
                return Err(Error::InvalidInput(
                    "image asset must be below images/".into(),
                ));
            };
            if leaf.is_empty()
                || leaf.starts_with('.')
                || !leaf
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
                || !names.insert(asset.filename.clone())
            {
                return Err(Error::InvalidInput(
                    "unsafe or duplicate image filename".into(),
                ));
            }
        }
        let markdown = doc.to_markdown();
        let json = doc.to_json()?;
        fs::create_dir(destination)?;
        // Leave partial new output on failure; never recursively delete it.
        fs::create_dir(destination.join("images"))?;
        for asset in &doc.assets {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination.join(&asset.filename))?
                .write_all(&asset.bytes)?;
        }
        for (name, bytes) in [
            ("document.md", markdown.as_bytes()),
            ("document.json", json.as_bytes()),
        ] {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination.join(name))?
                .write_all(bytes)?;
        }
        Ok(())
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use omniattach_documents::{Block, BlockKind, ExtractedAsset, Options, SourceSpan};
        fn document() -> Document {
            let mut doc =
                omniattach_documents::extract_bytes("empty.txt", b"", &Options::default()).unwrap();
            doc.assets.push(ExtractedAsset {
                id: "asset".into(),
                filename: "images/fixture.jpg".into(),
                media_type: "image/jpeg".into(),
                sha256: String::new(),
                byte_length: 3,
                bytes: vec![1, 2, 3],
                source: SourceSpan::default(),
            });
            doc.blocks.push(Block {
                id: "block".into(),
                parent_id: None,
                kind: BlockKind::Image {
                    asset_id: "asset".into(),
                },
                text: "Fixture".into(),
                section_path: Vec::new(),
                source: SourceSpan::default(),
                keep_together: true,
            });
            doc
        }
        #[test]
        fn writes_linked_bytes_and_refuses_overwrite() {
            let doc = document();
            let temp = tempfile::tempdir().unwrap();
            let output = temp.path().join("export");
            write_new(&doc, &output).unwrap();
            assert_eq!(
                fs::read(output.join(&doc.assets[0].filename)).unwrap(),
                doc.assets[0].bytes
            );
            assert_eq!(
                fs::read_to_string(output.join("document.md")).unwrap(),
                doc.to_markdown()
            );
            assert!(write_new(&doc, &output).is_err());
        }
        #[test]
        fn rejects_unsafe_paths_before_creating_output() {
            let mut doc = document();
            doc.assets[0].filename = "images/../../escape.jpg".into();
            let temp = tempfile::tempdir().unwrap();
            let output = temp.path().join("bad");
            assert!(write_new(&doc, &output).is_err());
            assert!(!output.exists());
        }
    }
}
