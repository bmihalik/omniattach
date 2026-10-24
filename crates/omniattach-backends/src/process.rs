use crate::{Error, Result};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub timeout: Duration,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub cancellation: CancellationToken,
}
impl Default for RunOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(600),
            max_stdout_bytes: 16 * 1024 * 1024,
            max_stderr_bytes: 16 * 1024 * 1024,
            cancellation: CancellationToken::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    /// Each entry is one argument; never evaluated by a shell.
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    /// False by default; only a small system environment allowlist is retained.
    pub inherit_env: bool,
    pub env: BTreeMap<OsString, OsString>,
}
impl CommandSpec {
    pub fn new(program: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: vec![],
            cwd: cwd.into(),
            inherit_env: false,
            env: BTreeMap::new(),
        }
    }
    pub fn arg(mut self, value: impl Into<OsString>) -> Self {
        self.args.push(value.into());
        self
    }
    pub fn args<I, S>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(values.into_iter().map(Into::into));
        self
    }
}

#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub elapsed: Duration,
}
impl ProcessOutput {
    pub fn stdout_text(&self) -> Result<&str> {
        std::str::from_utf8(&self.stdout).map_err(|e| Error::InvalidOutput(e.to_string()))
    }
    pub fn stderr_text(&self) -> Result<&str> {
        std::str::from_utf8(&self.stderr).map_err(|e| Error::InvalidOutput(e.to_string()))
    }
}

/// Injectable execution boundary. Implementors must enforce the provided limits/token.
pub trait Runner: Send + Sync {
    fn run(&self, spec: &CommandSpec, options: &RunOptions) -> Result<ProcessOutput>;
}
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessRunner;

#[cfg(unix)]
mod unix {
    use super::*;
    use std::{
        io::{ErrorKind, Read},
        os::{fd::AsRawFd, unix::process::CommandExt},
        process::{Child, Command, Stdio},
        time::Instant,
    };

    fn nonblocking(fd: &impl AsRawFd) -> Result<()> {
        // SAFETY: these calls only change flags on a valid, owned pipe descriptor.
        let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
    struct Group(Child);
    impl Drop for Group {
        fn drop(&mut self) {
            // SAFETY: the child is assigned its own process group at spawn; a negative id targets it.
            unsafe {
                libc::kill(-(self.0.id() as i32), libc::SIGKILL);
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn drain(
        reader: &mut impl Read,
        buffer: &mut Vec<u8>,
        limit: usize,
        label: &str,
    ) -> Result<bool> {
        let mut temp = [0; 8192];
        // Bounded work per poll preserves timeout/cancellation responsiveness on noisy tools.
        for _ in 0..8 {
            match reader.read(&mut temp) {
                Ok(0) => return Ok(true),
                Ok(n) => {
                    if buffer.len().saturating_add(n) > limit {
                        return Err(Error::Limit(label.into()));
                    }
                    buffer.extend_from_slice(&temp[..n]);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(false)
    }
    impl Runner for ProcessRunner {
        fn run(&self, spec: &CommandSpec, options: &RunOptions) -> Result<ProcessOutput> {
            if options.cancellation.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let start = Instant::now();
            let mut command = Command::new(&spec.program);
            command
                .args(&spec.args)
                .current_dir(&spec.cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .process_group(0);
            if !spec.inherit_env {
                command.env_clear();
                for key in ["PATH", "HOME", "TMPDIR", "XDG_RUNTIME_DIR", "SYSTEMROOT"] {
                    if let Some(value) = std::env::var_os(key) {
                        command.env(key, value);
                    }
                }
            }
            command
                .env("LC_ALL", "C")
                .env("GIO_USE_VFS", "local")
                .envs(&spec.env);
            let mut child = Group(command.spawn().map_err(|source| Error::Spawn {
                program: spec.program.clone(),
                source,
            })?);
            let mut stdout = child.0.stdout.take().expect("piped stdout");
            let mut stderr = child.0.stderr.take().expect("piped stderr");
            nonblocking(&stdout)?;
            nonblocking(&stderr)?;
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let (mut out_eof, mut err_eof) = (false, false);
            let mut status = None;
            loop {
                if options.cancellation.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                if start.elapsed() >= options.timeout {
                    return Err(Error::Timeout(
                        options.timeout.as_millis().min(u64::MAX as u128) as u64,
                    ));
                }
                if !out_eof {
                    out_eof = drain(&mut stdout, &mut out, options.max_stdout_bytes, "stdout")?;
                }
                if !err_eof {
                    err_eof = drain(&mut stderr, &mut err, options.max_stderr_bytes, "stderr")?;
                }
                if status.is_none() {
                    status = child.0.try_wait()?;
                }
                if let Some(status) = status
                    && out_eof
                    && err_eof
                {
                    if !status.success() {
                        return Err(Error::ProcessFailed {
                            code: status.code(),
                            stderr: tail(&err, 4096),
                        });
                    }
                    return Ok(ProcessOutput {
                        stdout: out,
                        stderr: err,
                        elapsed: start.elapsed(),
                    });
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

pub(crate) fn tail(bytes: &[u8], limit: usize) -> String {
    String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(limit)..]).into_owned()
}

#[cfg(not(unix))]
impl Runner for ProcessRunner {
    fn run(&self, _: &CommandSpec, _: &RunOptions) -> Result<ProcessOutput> {
        Err(Error::Unsupported(
            "ProcessRunner currently requires Unix process groups; inject a platform runner".into(),
        ))
    }
}
