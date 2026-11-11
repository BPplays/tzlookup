use log::{debug, trace};
use std::fs;
use std::io::{self, Write};
use std::os::linux::fs::MetadataExt;
use std::os::unix::fs::{symlink, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// Canonicalise `path`, checking for directory traversal outside of `base`.
///
/// Since we are linking based upon the output of some data we retrieved over the internet, we
/// should check that the output doesn't attempt to do something naughty with local path traversal.
///
/// This function checks that the base directory of the zoneinfo database shares a common prefix
/// with the absolute path of the requested zoneinfo file.
fn safe_canonicalise_path(
    base: impl AsRef<Path>,
    path: impl AsRef<Path>,
) -> io::Result<impl AsRef<Path>> {
    let base = base.as_ref();
    let path_ref = path.as_ref();

    if path_ref.canonicalize()?.starts_with(base.canonicalize()?) {
        Ok(path)
    } else {
        Err(io::Error::other(format!(
            "Directory traversal detected, {} is outside of {}",
            path_ref.display(),
            base.display()
        )))
    }
}

/// Given a file that we eventually want to atomically rename to, give us a temporary path that we
/// can use in the interim.
///
/// This must be in the same directory as `path` because we will later call `fs::rename` on it,
/// which is only atomic when not cross device.
fn get_tmp_path(path: &Path) -> io::Result<PathBuf> {
    tempfile::Builder::new()
        .tempfile_in(
            path.parent().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Refusing to create temp file in root",
                )
            })?,
        )?
        .into_temp_path()
        .canonicalize()
}

#[derive(Debug, thiserror::Error)]
pub enum LinkLocaltimeError {
    #[error("Failed to create temporary localtime path: {0}")]
    CreateTempPath(#[from] io::Error),

    #[error(
        "Timezone \"{timezone}\" requested, but this timezone is not available on your operating system"
    )]
    TimezoneNotFound { timezone: String },

    #[error("Failed to canonicalise timezone path")]
    CanonicaliseTimezone(#[source] io::Error),

    #[error("Failed to remove temporary localtime file")]
    RemoveTempFile(#[source] io::Error),

    #[error("Failed to create localtime symlink")]
    CreateSymlink(#[source] io::Error),

    #[error("Failed to read temporary localtime metadata")]
    ReadTempMetadata(#[source] io::Error),

    #[error("localtime path has no parent")]
    MissingLocaltimeParent,

    #[error("Failed to read localtime parent metadata")]
    ReadParentMetadata(#[source] io::Error),

    #[error(
        "Cannot atomically rename, {} and {} are not on the same device",
        .temporary_path.display(),
        .localtime_path.display()
    )]
    DifferentDevices {
        temporary_path: PathBuf,
        localtime_path: PathBuf,
    },

    #[error("Failed to atomically rename localtime file")]
    Rename(#[source] io::Error),
}

/// Atomically link to a timezone file from the zoneinfo db from `localtime_path`.
///
/// Since we may be retrieving the timezone file's relative path from an untrusted source, we also
/// do checks to make sure that no directory traversal is going on. See `safe_canonicalise_path`
/// for information about how that works.
pub fn link_localtime(
    timezone: &str,
    localtime_path: impl AsRef<Path>,
    zoneinfo_path: impl AsRef<Path>,
) -> std::result::Result<(), LinkLocaltimeError> {
    let localtime_path = localtime_path.as_ref();
    let zoneinfo_path = zoneinfo_path.as_ref();

    let localtime_tmp_path = get_tmp_path(localtime_path)
        .map_err(LinkLocaltimeError::CreateTempPath)?;

    let tz_path = match safe_canonicalise_path(
        zoneinfo_path,
        zoneinfo_path.join(timezone),
    ) {
        Ok(path) => path,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Err(LinkLocaltimeError::TimezoneNotFound {
                timezone: timezone.to_owned(),
            });
        }
        Err(err) => return Err(LinkLocaltimeError::CanonicaliseTimezone(err)),
    };

    if let Err(err) = fs::remove_file(&localtime_tmp_path) {
        if err.kind() != io::ErrorKind::NotFound {
            return Err(LinkLocaltimeError::RemoveTempFile(err));
        }
    }

    trace!(
        "Symlinking {} to {}",
        localtime_tmp_path.display(),
        tz_path.as_ref().display()
    );
    symlink(tz_path, &localtime_tmp_path)
        .map_err(LinkLocaltimeError::CreateSymlink)?;

    // We should seek to avoid /etc/localtime disappearing, even briefly, to avoid
    // applications being unhappy -- that's why we insist on atomic rename.
    let tmp_dev = localtime_tmp_path
        .metadata()
        .map_err(LinkLocaltimeError::ReadTempMetadata)?
        .st_dev();

    let target_dev = localtime_path
        .parent()
        .ok_or(LinkLocaltimeError::MissingLocaltimeParent)?
        .metadata()
        .map_err(LinkLocaltimeError::ReadParentMetadata)?
        .st_dev();

    if tmp_dev != target_dev {
        fs::remove_file(&localtime_tmp_path)
            .map_err(LinkLocaltimeError::RemoveTempFile)?;

        return Err(LinkLocaltimeError::DifferentDevices {
            temporary_path: localtime_tmp_path,
            localtime_path: localtime_path.to_owned(),
        });
    }

    trace!(
        "Atomically renaming {} to {}",
        localtime_tmp_path.display(),
        localtime_path.display()
    );

    fs::rename(localtime_tmp_path, localtime_path)
        .map_err(LinkLocaltimeError::Rename)?;

    Ok(())
}

/// Debian and derivatives also have /etc/timezone, which is used for a human readable timezone.
/// Without this, dpkg-reconfigure will nuke /etc/localtime on reconfigure.
///
/// If `always_write` is false, we will skip when /etc/timezone doesn't exist.
pub fn write_debian_timezone(
    timezone: &str,
    filename: impl AsRef<Path>,
    always_write: bool,
) -> io::Result<()> {
    let filename = filename.as_ref();

    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create(always_write)
        .truncate(true)
        .mode(0o644)
        .open(&filename)
    {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            debug!("{} does not exist, not writing to it", filename.display());
            return Ok(());
        }
        Err(err) => return Err(err),
    };
    let data = format!("{timezone}\n");
    file.write_all(data.as_bytes())
}


#[derive(Debug, thiserror::Error)]
pub enum SetTimezoneError {
    #[error(transparent)]
    LinkLocaltime(#[from] LinkLocaltimeError),

    #[error("Failed to update Debian timezone: {0}")]
    WriteDebian(#[from] io::Error),
}

/// Sets the timezone
pub fn set_timezone(
    timezone: &str,
    localtime_path: impl AsRef<Path>,
    zoneinfo_path: impl AsRef<Path>,
    debian_timezone_path: impl AsRef<Path>,
    always_write_debian_timezone: bool,
) -> Result<(), SetTimezoneError> {

    link_localtime(
        timezone,
        localtime_path,
        zoneinfo_path,
    )
    .map_err(SetTimezoneError::LinkLocaltime)?;

    write_debian_timezone(
        timezone,
        debian_timezone_path,
        always_write_debian_timezone,
    )
    .map_err(SetTimezoneError::WriteDebian)?;

    return Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalise_normal() {
        let base = PathBuf::from("/etc");
        let path = PathBuf::from("/etc/passwd");
        assert!(safe_canonicalise_path(base, path).is_ok());
    }

    #[test]
    fn canonicalise_back_and_forth() {
        let base = PathBuf::from("/etc");
        let path = PathBuf::from("/etc/../etc/passwd");
        assert!(safe_canonicalise_path(base, path).is_ok());
    }

    #[test]
    fn canonicalise_failure() {
        let base = PathBuf::from("/etc");
        let path = PathBuf::from("/etc/../passwd");
        assert!(safe_canonicalise_path(base, path).is_err());
    }

    #[test]
    fn link_localtime_missing_target() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        let zoneinfo_path = temp_path.join("zoneinfo");
        fs::create_dir_all(&zoneinfo_path).unwrap();
        let tz_file = zoneinfo_path.join("America").join("Toronto");
        fs::create_dir_all(tz_file.parent().unwrap()).unwrap();
        fs::write(&tz_file, "fake timezone data").unwrap();

        let localtime_path = temp_path.join("localtime");

        let result = link_localtime("America/Toronto", localtime_path.clone(), zoneinfo_path);
        assert!(
            result.is_ok(),
            "link_localtime should succeed when target doesn't exist: {:?}",
            result
        );

        assert!(localtime_path.exists());
        assert!(localtime_path.is_symlink());
    }
}
