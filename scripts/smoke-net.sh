#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Bela Istvan MIHALIK
# SPDX-License-Identifier: Apache-2.0

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$ROOT/target/tmp/smoke-net"
SAMPLES="$WORK/samples"
OUT="$WORK/out"

rm -rf "$OUT"
mkdir -p "$SAMPLES" "$OUT"

cargo build -p omniattach-cli --features backends
BIN="$ROOT/target/debug/omniattach"

curl -sSL "https://raw.githubusercontent.com/rust-lang/cargo/master/README.md" -o "$SAMPLES/sample.md"
curl -sSL "https://www.w3schools.com/xml/simple.xml" -o "$SAMPLES/menu.xml"
curl -sSL "https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf" -o "$SAMPLES/dummy.pdf"
curl -sSL "https://raw.githubusercontent.com/tesseract-ocr/tessdata/main/eng.traineddata" -r 0-4095 -o "$SAMPLES/head.bin"
curl -sSL "https://www.w3.org/People/mimasa/test/imgformat/img/w3c_home.png" -o "$SAMPLES/w3c.png"

echo "=== 1. Capabilities ==="
"$BIN" --external-backends capabilities --pretty

echo "=== 2. Format Probe ==="
"$BIN" probe "$SAMPLES/sample.md" --pretty
"$BIN" probe "$SAMPLES/menu.xml" --pretty
"$BIN" probe "$SAMPLES/dummy.pdf" --pretty
"$BIN" probe "$SAMPLES/w3c.png" --pretty

echo "=== 3. XML Structural Probe ==="
"$BIN" xml-probe "$SAMPLES/menu.xml" --pretty

echo "=== 4. Route Plan (Portable Text/Markdown) ==="
"$BIN" plan "$SAMPLES/sample.md" --goal markdown --pretty

echo "=== 5. Convert Markdown (In-Process Portable Route) ==="
"$BIN" convert "$SAMPLES/sample.md" \
    --goal markdown \
    --output "$OUT/md-bundle" \
    --progress ndjson \
    --pretty

echo "=== 6. Convert PDF via Poppler (External Route) ==="
"$BIN" --external-backends convert "$SAMPLES/dummy.pdf" \
    --goal extract \
    --route poppler \
    --allow-external \
    --output "$OUT/pdf-bundle" \
    --progress ndjson \
    --pretty

echo "=== 7. Convert PNG via Tesseract OCR (External Route) ==="
"$BIN" --external-backends convert "$SAMPLES/w3c.png" \
    --goal ocr \
    --route tesseract \
    --allow-external \
    --output "$OUT/ocr-bundle" \
    --progress ndjson \
    --pretty

echo "=== Exported Bundles in $OUT ==="
find "$OUT" -maxdepth 2 -type f

