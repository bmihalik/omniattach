//! Small table reconstruction core; coordinates are PDF user space.
#[derive(Clone, Debug)]
pub struct Run {
    pub bounds: [f64; 4], // left, bottom, right, top
    pub text: String,
    pub gap: bool,
    pub sequence: usize,
}
pub fn convert(xs: &[f64], ys: &[f64], runs: &[Run]) -> Result<String, String> {
    for axis in [xs, ys] {
        if axis.len() < 2 || axis.len() > 257 || axis.iter().any(|n| !n.is_finite())
            || axis.windows(2).any(|w| w[1] - w[0] <= 0.5) {
            return Err("grid needs 2..=257 finite, ascending boundaries separated by more than 0.5 PDF units".into());
        }
    }
    let columns = xs.len() - 1;
    let rows = ys.len() - 1;
    if columns * rows > 16384 { return Err("table cell limit exceeded".into()); }
    let mut cells: Vec<Vec<&Run>> = vec![Vec::new(); columns * rows];
    let mut text_bytes = 0usize;
    for run in runs {
        let [left, bottom, right, top] = run.bounds;
        if run.bounds.iter().any(|n| !n.is_finite()) || left >= right || bottom >= top {
            return Err(format!("invalid text geometry at sequence {}", run.sequence));
        }
        if right <= xs[0] || left >= xs[columns] || top <= ys[0] || bottom >= ys[rows] { continue; }
        if run.text.trim().is_empty() { continue; }
        text_bytes = text_bytes.checked_add(run.text.len()).ok_or("text size overflow")?;
        if text_bytes > 8 * 1024 * 1024 { return Err("table text limit exceeded".into()); }
        let x = xs.windows(2).position(|w| left >= w[0] && right <= w[1]);
        let y = ys.windows(2).position(|w| bottom >= w[0] && top <= w[1]);
        let (Some(x), Some(y)) = (x, y) else {
            return Err(format!("text crosses grid boundary at sequence {}; estimated bounds need review", run.sequence));
        };
        cells[(rows - 1 - y) * columns + x].push(run);
    }
    let mut output = String::new();
    for row in 0..rows {
        output.push('|');
        for column in 0..columns {
            let cell = &mut cells[row * columns + column];
            cell.sort_by(|a,b| b.bounds[3].total_cmp(&a.bounds[3]).then(a.bounds[0].total_cmp(&b.bounds[0])).then(a.sequence.cmp(&b.sequence)));
            let mut text = String::new();
            let mut previous: Option<&Run> = None;
            for run in cell.iter().copied() {
                if let Some(last) = previous {
                    if (last.bounds[3] - run.bounds[3]).abs() > 1.0 { text.push_str("<br>"); }
                    else if run.gap || run.bounds[0] - last.bounds[2] > 1.0 { text.push(' '); }
                }
                for c in run.text.chars() {
                    match c {
                        '&' => text.push_str("&amp;"), '<' => text.push_str("&lt;"), '>' => text.push_str("&gt;"),
                        '|' => text.push_str("&#124;"), '\\' => text.push_str("&#92;"),
                        '*' => text.push_str("&#42;"), '_' => text.push_str("&#95;"), '`' => text.push_str("&#96;"),
                        '[' => text.push_str("&#91;"), ']' => text.push_str("&#93;"),
                        '\n' => text.push_str("<br>"), '\r' => {}, c => text.push(c),
                    }
                }
                previous = Some(run);
            }
            output.push(' '); output.push_str(&text); output.push_str(" |");
        }
        output.push('\n');
        if row == 0 { output.push('|'); for _ in 0..columns { output.push_str(" --- |"); } output.push('\n'); }
    }
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(bounds: [f64;4], text: &str) -> Run { Run { bounds, text: text.into(), gap: false, sequence: 1 } }
    #[test] fn reconstructs_top_down_and_preserves_empty_cells() {
        let runs = [run([1.,11.,9.,19.], "Header"), run([11.,1.,19.,9.], "Value")];
        assert_eq!(convert(&[0.,10.,20.], &[0.,10.,20.], &runs).unwrap(), "| Header |  |\n| --- | --- |\n|  | Value |\n");
    }
    #[test] fn rejects_crossing_text_and_invalid_grids() {
        assert!(convert(&[0.,10.,20.], &[0.,10.], &[run([9.,1.,11.,9.],"bad")]).is_err());
        assert!(convert(&[0.,0.], &[0.,10.], &[]).is_err());
        assert!(convert(&[0.,f64::NAN], &[0.,10.], &[]).is_err());
    }
    #[test] fn escapes_markdown_and_html() {
        let md = convert(&[0.,10.], &[0.,10.], &[run([1.,1.,9.,9.],"a|<b>&_")]).unwrap();
        assert!(md.contains("a&#124;&lt;b&gt;&amp;&#95;"));
    }
    #[test] fn joins_fragments_and_multiple_lines() {
        let runs = [run([1.,11.,3.,19.],"A"), run([5.,11.,8.,19.],"B"),run([1.,1.,8.,9.],"C")];
        assert!(convert(&[0.,10.], &[0.,20.], &runs).unwrap().contains("A B<br>C"));
    }
}
