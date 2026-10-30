use omniattach_documents::{Error, Format, Options, detect_format, extract_bytes};

#[test]
fn ebook_extensions_are_case_insensitive() {
    assert_eq!(detect_format("BOOK.MOBI").unwrap(), Format::Mobi);
    assert_eq!(detect_format("BOOK.EPUB").unwrap(), Format::Epub);
    assert_eq!(detect_format("BOOK.AZW3").unwrap(), Format::Azw3);
}

#[cfg(not(feature = "mobi"))]
#[test]
fn mobi_requires_its_feature() {
    assert!(matches!(extract_bytes("book.mobi", b"", &Options::default()), Err(Error::FeatureDisabled(_))));
}

#[cfg(feature = "mobi")]
#[test]
fn malformed_mobi_is_not_treated_as_text() {
    assert!(matches!(extract_bytes("book.mobi", b"plain text", &Options::default()), Err(Error::InvalidInput(_))));
}

#[cfg(not(feature = "azw3"))]
#[test]
fn azw3_requires_its_feature() {
    assert!(matches!(extract_bytes("book.azw3", b"", &Options::default()), Err(Error::FeatureDisabled(_))));
}

#[cfg(feature = "office")]
#[test]
fn malformed_epub_is_not_treated_as_text() {
    assert!(extract_bytes("book.epub", b"not a ZIP", &Options::default()).is_err());
}

#[test]
fn input_limit_applies_before_ebook_parsing() {
    let mut options = Options::default();
    options.limits.input_bytes = 1;
    assert!(matches!(extract_bytes("book.mobi", b"too big", &options), Err(Error::LimitExceeded(_))));
}
