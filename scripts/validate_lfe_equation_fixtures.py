#!/usr/bin/env python3
"""Independent PDF parser/renderer checks. This is not an OCR accuracy test."""
import argparse
import hashlib
import json
from pathlib import Path

import pypdfium2 as pdfium
from pypdf import PdfReader
from pypdf.generic import ContentStream


def operators(page, reader):
    return [op for _, op in ContentStream(page.get_contents(), reader).operations]


def validate(root):
    if not __debug__:
        raise RuntimeError("validation must not be run with Python -O")
    root = Path(root)
    m = json.loads((root / "manifest.json").read_text())
    assert m["schema_version"] == "omniattach/lfe-equation-fixtures/0.1"
    docs = {x["path"]: x for x in m["documents"]}
    assert len(docs) == len(m["documents"]) == (9 if m["latex_enabled"] else 8)
    assert len({x["id"] for x in m["occurrences"]}) == len(m["occurrences"])
    for item in m["documents"] + m["assets"]:
        path = root / item["path"]
        assert path.resolve().is_relative_to(root.resolve())
        assert hashlib.sha256(path.read_bytes()).hexdigest() == item["sha256"], item["path"]
    readers = {name: PdfReader(root/name) for name in docs}
    for name, reader in readers.items():
        assert len(reader.pages) == docs[name]["pages"]
        assert not docs[name]["negative"] or not any(o["pdf"] == name for o in m["occurrences"])
        rendered = pdfium.PdfDocument(str(root/name))
        try:
            for page in rendered:
                img = page.render(scale=1).to_pil().convert("L")
                assert img.getextrema()[0] < 200, (name, "blank page")
        finally:
            rendered.close()
    formulas = {x["id"]: x["latex"] for x in m["source_formulas"]}
    for occurrence in m["occurrences"]:
        name = occurrence["pdf"]
        page = readers[name].pages[occurrence["page"]-1]
        assert occurrence["bbox_kind"] == "allocated_region_not_ink"
        x0,y0,x1,y1 = occurrence["bbox_pdf"]
        assert 0 <= x0 < x1 <= 612 and 0 <= y0 < y1 <= 792
        assert occurrence["latex"].strip()
        assert occurrence["latex"] == formulas[occurrence["formula_id"]]
        # Normalize only a COPY for this check: regions use unrotated media coordinates.
        writer = __import__('pypdf').PdfWriter()
        import io
        writer.add_page(page)
        normalized = writer.pages[0]
        normalized.rotation = 0
        normalized.cropbox = normalized.mediabox
        stream = io.BytesIO()
        writer.write(stream)
        rendered = pdfium.PdfDocument(stream.getvalue())
        try:
            image = rendered[0].render(scale=1).to_pil().convert("L")
            crop = image.crop((int(x0), int(792-y1), int(x1), int(792-y0)))
            assert crop.getextrema()[0] < 160, (occurrence["id"], "blank expected region")
        finally:
            rendered.close()
    native = readers["native.pdf"]
    assert all(any(op in [b"Tj",b"TJ"] for op in operators(p,native)) for p in native.pages)
    outline = readers["outlines.pdf"]
    assert all(b"m" in operators(p,outline) and not any(op in [b"Tj",b"TJ"] for op in operators(p,outline)) for p in outline.pages)
    for page in readers["raster.pdf"].pages:
        objects = page["/Resources"]["/XObject"]
        assert len(objects) == 1
        image = next(iter(objects.values())).get_object()
        assert image["/ColorSpace"] == "/DeviceRGB" and image["/BitsPerComponent"] == 8
        assert image["/Filter"] in (["/FlateDecode"], "/FlateDecode")
    repeated = readers["repeated.pdf"]
    cs = ContentStream(repeated.pages[0].get_contents(), repeated)
    uses = [str(args[0]) for args,op in cs.operations if op == b"Do"]
    assert len(uses)==2 and uses[0]==uses[1], "not one XObject painted twice"
    for page in readers["scanned.pdf"].pages:
        assert not page.extract_text().strip(), "scan contains a text layer"
        objects = page["/Resources"]["/XObject"]
        assert len(objects)==1 and next(iter(objects.values())).get_object()["/Subtype"] == "/Image"
    transformed = readers["page-transform.pdf"].pages[0]
    assert transformed.rotation == 90 and list(transformed.cropbox) == [24,36,588,756]
    return dict(ok=True, documents=len(docs), occurrences=len(m["occurrences"]),
        scope="hashes, PDF structure, independent rendering and nonblank allocated regions; not exact ink bounds, recognition or Rust integration")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("corpus", type=Path)
    print(json.dumps(validate(parser.parse_args().corpus), indent=2))
