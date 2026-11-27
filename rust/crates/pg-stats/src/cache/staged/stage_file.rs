use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use tempfile::TempPath;

pub const STAGE_CLEANUP_ARG: &str = "--internal-stats-stage-cleanup";
const PREFIX: &str = "pangloss-stats-";

pub(super) enum StageFile {
    Local(TempPath),
    Supervised { path: PathBuf, _keeper: Keeper },
}

fn create_stage(directory: &Path) -> io::Result<TempPath> {
    Ok(tempfile::Builder::new()
        .prefix(PREFIX)
        .tempfile_in(directory)?
        .into_temp_path())
}

impl StageFile {
    pub(super) fn local(directory: &Path) -> io::Result<Self> {
        Ok(Self::Local(create_stage(directory)?))
    }

    pub(super) fn supervised(directory: &Path) -> io::Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg(STAGE_CLEANUP_ARG)
            .arg(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        // The keeper must survive a terminal interrupt of the batch.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        #[cfg(windows)]
        {
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            std::os::windows::process::CommandExt::creation_flags(
                &mut command,
                CREATE_NEW_PROCESS_GROUP,
            );
        }
        let mut keeper = Keeper(command.spawn()?);
        let mut name = String::new();
        BufReader::new(keeper.0.stdout.take().expect("piped stage name")).read_line(&mut name)?;
        let name = name.trim_end_matches(['\r', '\n']);
        if !name.starts_with(PREFIX)
            || !name
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-')
        {
            return Err(io::Error::other(
                "stats staging process did not return a temporary file name",
            ));
        }
        Ok(Self::Supervised {
            path: directory.join(name),
            _keeper: keeper,
        })
    }

    pub(super) fn path(&self) -> &Path {
        match self {
            Self::Local(path) => path,
            Self::Supervised { path, .. } => path,
        }
    }
}

pub(super) struct Keeper(Child);

impl Drop for Keeper {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        let _ = self.0.wait();
    }
}

/// Owns the staging pathname until the batch closes its pipe or exits.
pub fn clean_up_stage(directory: &Path) -> io::Result<()> {
    let path = create_stage(directory)?;
    let mut stdout = io::stdout().lock();
    writeln!(
        stdout,
        "{}",
        path.file_name().expect("stage file name").to_string_lossy()
    )?;
    stdout.flush()?;
    drop(stdout);
    io::copy(&mut io::stdin().lock(), &mut io::sink())?;
    #[cfg(windows)]
    {
        // An interrupted process can close the pipe before Windows closes its SQLite handles.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match std::fs::remove_file(&path) {
                Err(error)
                    if error.raw_os_error() == Some(32) && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => return Ok(()),
            }
        }
    }
    #[cfg(not(windows))]
    path.close()
}
