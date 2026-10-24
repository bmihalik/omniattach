//! Opt-in real-tool smoke tests. No downloads or speech model needed.
#![cfg(unix)]
use omniattach_backends::*;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn run(args: &[&str]) {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn video(root: &Path) -> PathBuf {
    let path = root.join("source.mkv");
    run(&[
        "-f",
        "lavfi",
        "-i",
        "color=c=black:s=96x64:r=10:d=1",
        "-f",
        "lavfi",
        "-i",
        "color=c=white:s=96x64:r=10:d=1",
        "-filter_complex",
        "[0:v][1:v]concat=n=2:v=1:a=0[v]",
        "-map",
        "[v]",
        "-c:v",
        "ffv1",
        path.to_str().unwrap(),
    ]);
    path
}
#[test]
#[ignore = "requires installed ffmpeg and ffprobe; run explicitly"]
fn real_audio_normalization_and_probe() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("tone.wav");
    run(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=0.2",
        "-ar",
        "44100",
        "-ac",
        "2",
        source.to_str().unwrap(),
    ]);
    let ctx = BackendContext::new(&ProcessRunner);
    let audio = FfmpegAudio::default().convert(&source, &ctx).unwrap();
    let wav = audio.asset_path(0).unwrap();
    let probe = Ffprobe::default().convert(&wav, &ctx).unwrap();
    let stream = &probe.manifest.metadata["streams"][0];
    assert_eq!(stream["sample_rate"], "16000");
    assert_eq!(stream["channels"], 1);
    assert_eq!(stream["codec_name"], "pcm_s16le");
}
#[test]
#[ignore = "requires installed ffmpeg; run explicitly"]
fn real_scene_cut_frames_include_first_frame_and_transition() {
    let temp = tempfile::tempdir().unwrap();
    let source = video(temp.path());
    let backend = FfmpegFrames {
        selection: FrameSelection::SceneCuts { threshold: 0.2 },
        width: 96,
        ..Default::default()
    };
    let result = backend
        .convert(&source, &BackendContext::new(&ProcessRunner))
        .unwrap();
    let times: Vec<_> = result
        .manifest
        .assets
        .iter()
        .map(|a| a.location.start_ms.unwrap())
        .collect();
    assert_eq!(times, vec![0, 1000]);
    assert!(result.asset_path(1).unwrap().is_file());
}
#[test]
#[ignore = "requires installed ffmpeg; run explicitly"]
fn real_interval_frames_keep_pts_and_frame_cap_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    let source = video(temp.path());
    let backend = FfmpegFrames {
        selection: FrameSelection::Interval { interval_ms: 500 },
        max_frames: 3,
        width: 96,
        ..Default::default()
    };
    let result = backend
        .convert(&source, &BackendContext::new(&ProcessRunner))
        .unwrap();
    let times: Vec<_> = result
        .manifest
        .assets
        .iter()
        .map(|a| a.location.start_ms.unwrap())
        .collect();
    assert_eq!(times, vec![0, 500, 1000]);
    assert!(
        result
            .manifest
            .diagnostics
            .iter()
            .any(|d| d.code == "FRAME_LIMIT_REACHED")
    );
}
fn pdf() -> Vec<u8> {
    let stream = "BT /F1 12 Tf 20 100 Td (Hello omniattach) Tj ET";
    let objects=["<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream",stream.len()),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] >>".into()];
    let mut data = String::from("%PDF-1.4\n");
    let mut offsets = vec![0];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(data.len());
        data.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
    }
    let xref = data.len();
    data.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for offset in offsets.iter().skip(1) {
        data.push_str(&format!("{offset:010} 00000 n \n"));
    }
    data.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    data.into_bytes()
}
#[test]
#[ignore = "requires installed pdftotext; run explicitly"]
fn real_pdf_page_provenance_and_blank_page() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("two.pdf");
    std::fs::write(&source, pdf()).unwrap();
    let result = Poppler::default()
        .convert(&source, &BackendContext::new(&ProcessRunner))
        .unwrap();
    assert!(result.manifest.text[0].text.contains("Hello omniattach"));
    assert_eq!(result.manifest.text.len(), 2);
    assert_eq!(result.manifest.text[1].location.page, Some(2));
    assert!(result.manifest.text[1].text.is_empty());
}
#[test]
#[ignore = "requires installed libreoffice and pdftotext; run explicitly"]
fn real_libreoffice_converts_fodp_in_isolated_profile() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("sample.fodp");
    std::fs::write(&source, r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:mimetype="application/vnd.oasis.opendocument.presentation" office:version="1.3">
<office:automatic-styles><style:page-layout style:name="PM1"><style:page-layout-properties fo:page-width="28cm" fo:page-height="21cm" style:print-orientation="landscape"/></style:page-layout></office:automatic-styles>
<office:master-styles><style:master-page style:name="Default" style:page-layout-name="PM1"/></office:master-styles>
<office:body><office:presentation><draw:page draw:name="page1" draw:master-page-name="Default"><draw:frame svg:x="1cm" svg:y="1cm" svg:width="20cm" svg:height="4cm"><draw:text-box><text:p>Hello omniattach Office</text:p></draw:text-box></draw:frame></draw:page></office:presentation></office:body></office:document>"#).unwrap();
    let ctx = BackendContext::new(&ProcessRunner);
    let converted = LibreOffice::new(OfficeFormat::Pdf)
        .convert(&source, &ctx)
        .unwrap();
    let extracted = Poppler::default()
        .convert(&converted.asset_path(0).unwrap(), &ctx)
        .unwrap();
    assert!(
        extracted
            .manifest
            .text
            .iter()
            .any(|t| t.text.contains("Hello omniattach Office"))
    );
}
