#!/usr/bin/env bash
set -euo pipefail

EXPECTED_TEXT="${OMNIATTACH_EXPECTED_OCR_TEXT:-OmniAttachOcrCheck}"

for cmd in pdftoppm tesseract cargo; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "error: required command not found: $cmd" >&2
        exit 1
    fi
done

if command -v magick >/dev/null 2>&1; then
    IM_CMD="magick"
elif command -v convert >/dev/null 2>&1; then
    IM_CMD="convert"
else
    echo "error: ImageMagick (magick or convert) not found" >&2
    exit 1
fi

TMP_DIR="$(mktemp -d -t omniattach-ocr-test-XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

PNG_FILE="$TMP_DIR/page.png"
PDF_FILE="$TMP_DIR/scan.pdf"

"$IM_CMD" -size 1200x400 xc:white \
    -fill black -pointsize 72 -gravity center \
    -annotate +0+0 "$EXPECTED_TEXT" \
    -density 300 -units PixelsPerInch \
    "$PNG_FILE"

if command -v img2pdf >/dev/null 2>&1; then
    img2pdf "$PNG_FILE" -o "$PDF_FILE"
elif command -v tiff2pdf >/dev/null 2>&1; then
    TIFF_FILE="$TMP_DIR/page.tiff"
    "$IM_CMD" "$PNG_FILE" "$TIFF_FILE"
    tiff2pdf -o "$PDF_FILE" "$TIFF_FILE"
else
    "$IM_CMD" "$PNG_FILE" "$PDF_FILE"
fi

echo "Fabricated image-only PDF: $PDF_FILE (expected text: '$EXPECTED_TEXT')"

OMNIATTACH_SCANNED_PDF="$PDF_FILE" \
OMNIATTACH_EXPECTED_OCR_TEXT="$EXPECTED_TEXT" \
cargo test -p omniattach --features workspace --test pdf_ocr_route -- --ignored --nocapture
