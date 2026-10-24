use omniattach_documents::{
    CsvOptions, Options, Repeated, XmlColumn, XmlTableSpec, extract_xml_table,
};
fn main() -> omniattach_documents::Result<()> {
    let spec = XmlTableSpec::new(
        "/m:measurements/m:sample",
        vec![
            XmlColumn::new("id", "@id"),
            XmlColumn::new("material", "m:material"),
            XmlColumn::new("value", "m:value"),
            XmlColumn::new("unit", "m:value/@unit"),
            XmlColumn::new("tags", "m:tag").repeated(Repeated::Json),
        ],
    )
    .namespace("m", "urn:measurement");
    let document = extract_xml_table(
        "measurements.xml",
        include_bytes!("../tests/data/measurements.xml"),
        spec,
        &Options::default(),
    )?;
    print!(
        "{}",
        document.tables[0].to_csv_with(&CsvOptions {
            null: "\\N".into(),
            ..Default::default()
        })?
    );
    Ok(())
}
