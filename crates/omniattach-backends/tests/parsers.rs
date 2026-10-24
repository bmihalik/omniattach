use omniattach_backends::*;
#[test]
fn poppler_retains_blank_pages_and_one_based_positions() {
    let pages = parse_pdf_pages("First\u{c}\u{c}Third\u{c}");
    assert_eq!(pages.len(), 3);
    assert_eq!(pages[1].text, "");
    assert_eq!(pages[2].location.page, Some(3));
    assert!(parse_pdf_pages("").is_empty());
}
#[test]
fn whisper_cpp_offsets_are_already_milliseconds() {
    let units = parse_transcript(
        br#"{"transcription":[{"text":"  Hello  ","offsets":{"from":120,"to":900}}]}"#,
        TranscriptFormat::WhisperCpp,
    )
    .unwrap();
    assert_eq!(units[0].text, "Hello");
    assert_eq!(units[0].location.start_ms, Some(120));
}
#[test]
fn whisper_segments_seconds_are_rounded_to_milliseconds() {
    let units = parse_transcript(
        br#"{"segments":[{"text":"Hi","start":1.2345,"end":2.0}]}"#,
        TranscriptFormat::WhisperSegments,
    )
    .unwrap();
    assert_eq!(units[0].location.start_ms, Some(1235));
    assert_eq!(units[0].location.end_ms, Some(2000));
}
#[test]
fn malformed_transcript_contracts_fail_loudly() {
    for input in [
        r#"{}"#,
        r#"{"segments":{}}"#,
        r#"{"segments":[{"text":4,"start":0,"end":1}]}"#,
        r#"{"segments":[{"text":"a","start":-1,"end":1}]}"#,
        r#"{"segments":[{"text":"a","start":2,"end":1}]}"#,
        r#"{"segments":[{"text":"a","start":1e30,"end":1e30}]}"#,
    ] {
        assert!(
            parse_transcript(input.as_bytes(), TranscriptFormat::WhisperSegments).is_err(),
            "{input}"
        );
    }
    assert!(
        parse_transcript(
            br#"{"transcription":[{"text":"a"}]}"#,
            TranscriptFormat::WhisperCpp
        )
        .is_err()
    );
}
#[test]
fn empty_speech_is_valid_but_not_a_fake_text_unit() {
    assert!(
        parse_transcript(br#"{"segments":[]}"#, TranscriptFormat::WhisperSegments)
            .unwrap()
            .is_empty()
    );
    assert!(
        parse_transcript(
            br#"{"segments":[{"text":" ","start":0,"end":0}]}"#,
            TranscriptFormat::WhisperSegments
        )
        .unwrap()
        .is_empty()
    );
}
#[test]
fn tesseract_keeps_word_boxes_and_native_confidence() {
    let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n1\t1\t0\t0\t0\t0\t0\t0\t80\t40\t-1\t\n5\t2\t1\t1\t1\t1\t5\t7\t10\t11\t91.25\tárvíz\n";
    let words = parse_tesseract_tsv(tsv).unwrap();
    assert_eq!(words.len(), 1);
    assert_eq!(words[0].text, "árvíz");
    assert_eq!(words[0].confidence, Some(91.25));
    assert_eq!(words[0].location.page, Some(2));
    assert_eq!(words[0].location.bbox.as_ref().unwrap().left, 5);
    assert!(parse_tesseract_tsv(&tsv.replace("91.25", "NaN")).is_err());
    assert!(parse_tesseract_tsv(&tsv.replace("91.25", "101")).is_err());
    assert!(parse_tesseract_tsv("bad header").is_err());
}
#[test]
fn frame_times_are_pts_based_not_frame_rate_derived() {
    let text = "noise pts_time:99\n[Parsed_showinfo_3 @ 0] n:0 pts:0 pts_time:0 duration_time:0.04\n[Parsed_showinfo_3 @ 0] n:1 pts:0 pts_time:1.276 duration_time:0.04\n";
    assert_eq!(parse_frame_times(text).unwrap(), vec![0, 1276]);
    assert!(parse_frame_times("showinfo pts_time:N/A").is_err());
    assert!(parse_frame_times("showinfo pts_time:2\nshowinfo pts_time:1").is_err());
}
#[test]
fn one_blank_pdf_page_is_not_confused_with_empty_output() {
    let pages = parse_pdf_pages("\u{c}");
    assert_eq!(pages.len(), 1);
    assert!(pages[0].text.is_empty());
}
