use clap::Parser;
use omniattach_cli::{
    CliError, Runtime,
    args::{Cli, Command},
    service::{Engine, EngineConfig, InspectionOnlyEngine},
};
use std::io::Write;

fn run(cli: &Cli) -> omniattach_cli::Result<()> {
    let config = EngineConfig::from_cli(cli)?;
    let inspect = matches!(cli.command, Command::Probe(_) | Command::XmlProbe(_));
    let (engine, runtime): (Box<dyn Engine>, Runtime) = if inspect {
        // Inspection does not build a conversion engine or probe tools/models.
        (Box::new(InspectionOnlyEngine), Runtime::default())
    } else {
        #[cfg(feature = "facade")]
        {
            let engine = omniattach_cli::facade::FacadeEngine::new(&config)?;
            let runtime = engine.runtime();
            (Box::new(engine), runtime)
        }
        #[cfg(not(feature = "facade"))]
        {
            if config.external_backends {
                return Err(CliError::unsupported(
                    "external routes require the facade/backends Cargo features",
                ));
            }
            (Box::new(InspectionOnlyEngine), Runtime::default())
        }
    };
    let cancellation = runtime.clone();
    ctrlc::set_handler(move || cancellation.cancel())
        .map_err(|error| CliError::new("SIGNAL_HANDLER", error.to_string(), 1))?;
    let mut stdout = std::io::stdout().lock();
    omniattach_cli::dispatch(cli, engine.as_ref(), &runtime, &mut stdout)?;
    stdout.flush()?;
    Ok(())
}
fn main() -> std::process::ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return std::process::ExitCode::SUCCESS;
            }
            let failure = CliError::usage(error.to_string());
            emit(&failure);
            return std::process::ExitCode::from(failure.exit);
        }
    };
    match run(&cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            emit(&error);
            std::process::ExitCode::from(error.exit)
        }
    }
}
fn emit(error: &CliError) {
    let mut stderr = std::io::stderr().lock();
    let _ = serde_json::to_writer(&mut stderr, &error.as_json());
    let _ = stderr.write_all(b"\n");
}
