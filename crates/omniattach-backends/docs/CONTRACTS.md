# Command contracts

| Tool | Principal argv contract | Parse/acceptance |
|---|---|---|
| Poppler | `pdftotext -enc UTF-8 -eol unix [-layout] INPUT -` | Strict UTF-8, form-feed page boundaries; one trailing delimiter removed |
| Tesseract | `tesseract INPUT stdout -l LANGUAGE [--psm N] tsv` | Exact TSV header, level-5 words, page/boxes/confidence |
| LibreOffice | `libreoffice -env:UserInstallation=file:///PROFILE --headless --norestore --nodefault --nofirststartwizard --convert-to EXT --outdir OUT INPUT` | Expected `out/input.EXT`, regular contained file, ZIP/PDF signature |
| FFmpeg audio | `ffmpeg ... -protocol_whitelist file,pipe -i INPUT -map 0:a:0 -vn -ac 1 -ar 16000 -c:a pcm_s16le -f wav audio.wav` | RIFF/WAVE signature and output size |
| whisper.cpp | `whisper-cli -f audio.wav --output-json --output-file transcript -l LANGUAGE -m MODEL` | `transcript.json`, `transcription[].text`, `offsets.from/to` integer milliseconds |
| FFprobe | `ffprobe -v error -protocol_whitelist file,pipe -show_format -show_streams -of json INPUT` | JSON object |
| FFmpeg frames | first video stream, `setpts=PTS-STARTPTS,select=...,scale=...,showinfo`, `-fps_mode vfr`, `-frames:v N` | Declared PNG sequence and showinfo PTS mapping |

The source may be renamed to `input.EXT`; formats dependent on adjacent sidecars/resources are intentionally not reconstructed.

## Text parsing

PDF page numbering is one-based. Blank page text is retained and diagnosed. A successful zero-byte Poppler response is a no-text diagnostic, not fake OCR output. Page segmentation is based on Poppler's form-feed convention and is not a separate authoritative PDF page-count parse.

Tesseract word confidence is native 0..100, not a probability. -1 becomes `None`. Pixel coordinates refer to the supplied image, not original PDF points. Page/block/paragraph/line group numbers are not currently retained beyond the page number and record order. No OCR PDF rasterizer is implicitly invoked.

Whisper records with negative, missing or reversed timestamps fail. Empty speech arrays are valid and nonempty text is trimmed. Transcript chronology/overlap is otherwise preserved, not forced to a non-overlapping timeline. `WhisperSegments` rounds numeric seconds to milliseconds. The crate does not guess the JSON dialect from field names.

Frame timestamps are strictly parsed from FFmpeg showinfo output; malformed/missing/nonmonotonic timestamps fail. Files are numbered sequentially and tied to selected decoded frames. FFmpeg may evaluate extra selected frames after reaching its encoder cap; extra log records are diagnosed and ignored when they exceed actual emitted files. This version relies on FFmpeg's log contract, so validate it when upgrading the binary. It neither estimates timestamps from output filenames nor labels sample frames as complete semantic scenes.

## Custom outputs

A wrapper can return UTF-8 Markdown on stdout, a specific contained Markdown file, transcript JSON on stdout/file, or a generic declared file. The custom output path must be exactly specified and relative to the work directory (e.g. `out/document.md`). No recursive output guessing, auto-download, remote invocation or image relocation is performed by the adapter.

For faster-whisper, use a wrapper you control rather than assuming a canonical CLI is part of its Python package. For Marker/MinerU, pin and test the deployed release/wrapper; page markers and Markdown image paths vary and are not treated as reliable original provenance by this crate.
