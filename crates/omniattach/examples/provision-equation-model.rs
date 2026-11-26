//! Explicit Hub provisioning. Omit CACHE to use ModelStore::from_env().
use omniattach::{model_progress::*, model_store::*};
use std::{
    io::{self, IsTerminal, Write},
    time::Duration,
};

fn bytes(n: u64) -> String {
    let mut value = n as f64;
    let units = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", units[unit])
}
fn progress_line(e: &ModelProgressEvent) -> String {
    let mut line = format!("{:?}", e.stage);
    if let Some(file) = &e.file {
        line.push_str(&format!(
            " [{}/{}] {}  {}",
            e.file_index.unwrap_or(0),
            e.file_count,
            file,
            bytes(e.file_bytes)
        ));
        if let Some(total) = e.total_bytes.filter(|n| *n > 0) {
            let fraction = e.file_bytes.min(total) as f64 / total as f64;
            let filled = (fraction * 20.0).floor() as usize;
            line.push_str(&format!(
                " / {} [{}{}] {:3.0}%",
                bytes(total),
                "#".repeat(filled),
                "-".repeat(20 - filled),
                fraction * 100.0
            ));
        }
    }
    if let Some(message) = &e.message {
        line.push_str(&format!(": {message}"));
    }
    line.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut mode = "auto".to_string();
    let mut repo = None;
    let mut revision = None;
    let mut files = Vec::new();
    let mut refresh = false;
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--refresh" {
            refresh = true;
            args.remove(index);
            continue;
        }
        if matches!(
            args[index].as_str(),
            "--progress" | "--repo" | "--revision" | "--file"
        ) {
            if index + 1 >= args.len() {
                return Err("option requires a value".into());
            }
            let key = args.remove(index);
            let value = args.remove(index);
            match key.as_str() {
                "--progress" => mode = value,
                "--repo" => repo = Some(value),
                "--revision" => revision = Some(value),
                _ => files.push(value),
            }
        } else {
            index += 1;
        }
    }
    if !matches!(mode.as_str(), "auto" | "bar" | "text" | "ndjson" | "none") || args.is_empty() {
        return Err("usage: provision-equation-model [CACHE | --default-cache] --inspect | [CACHE | --default-cache] --download --allow-network [--progress auto|bar|text|ndjson|none] [--repo ID --revision REF --file PATH...]".into());
    }
    let store = if args[0] == "--default-cache" {
        args.remove(0);
        ModelStore::from_env()?
    } else if !args[0].starts_with('-') {
        ModelStore::new(args.remove(0))?
    } else {
        ModelStore::from_env()?
    };
    let mut spec = texteller3_onnx_spec()?;
    if let Some(repo_id) = repo {
        if files.is_empty() {
            return Err("custom Hub repo requires explicit --file selections".into());
        }
        spec = HubModelSpec {
            repo_id,
            revision: "main".into(),
            files: Vec::new(),
        };
    }
    if let Some(revision) = revision {
        spec.revision = revision;
    }
    if !files.is_empty() {
        spec.files = files
            .into_iter()
            .map(|path| HubFile { path, sha256: None })
            .collect();
    }
    let mut options = ProvisionOptions {
        timeout: Duration::from_secs(1800),
        refresh,
        ..Default::default()
    };
    if args.len() == 1 && args[0] == "--inspect" {
        println!(
            "{}",
            serde_json::to_string_pretty(&store.inspect(&spec, &options)?)?
        );
        return Ok(());
    }
    if args.len() != 2 || args[0] != "--download" || args[1] != "--allow-network" {
        return Err("Hub download requires --download --allow-network".into());
    }
    options.allow_network = true;
    let interactive = io::stderr().is_terminal() && matches!(mode.as_str(), "auto" | "bar");
    let observer = |event: &ModelProgressEvent| {
        if mode == "none" {
            return;
        }
        let mut err = io::stderr().lock();
        if mode == "ndjson" {
            let _ = serde_json::to_writer(&mut err, event);
            let _ = writeln!(err);
        } else if interactive {
            let line = progress_line(event);
            let width = std::env::var("COLUMNS")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(100)
                .clamp(20, 240);
            let line: String = line.chars().take(width.saturating_sub(1)).collect();
            let _ = write!(err, "\r\x1b[2K{line}");
            if !matches!(event.stage, ModelProvisionStage::Downloading) {
                let _ = writeln!(err);
            }
            let _ = err.flush();
        } else {
            let _ = writeln!(err, "{}", progress_line(event));
        }
    };
    let installed = store.provision_with_progress(&spec, &options, &observer)?;
    println!("{}", serde_json::to_string_pretty(&installed)?);
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(
                io::stderr().lock(),
                "{}",
                serde_json::json!({"schema_version":"omniattach/provision-error/0.2","error":error.to_string()})
            );
            std::process::ExitCode::FAILURE
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn human_sizes() {
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(0), "0.0 B");
    }
    #[test]
    fn unknown_total_has_no_percentage() {
        let mut e = ModelProgressEvent {
            schema_version: "omniattach/model-progress/0.2",
            model_id: "owner/model".into(),
            revision: "main".into(),
            stage: ModelProvisionStage::Downloading,
            file: Some("weights".into()),
            file_index: Some(1),
            file_count: 1,
            file_bytes: 50,
            downloaded_bytes: 50,
            total_bytes: None,
            elapsed_ms: 1,
            message: None,
        };
        assert!(!progress_line(&e).contains('%'));
        e.total_bytes = Some(100);
        assert!(progress_line(&e).contains("50%"));
        e.file_bytes = 200;
        assert!(progress_line(&e).contains("100%"));
    }
}
