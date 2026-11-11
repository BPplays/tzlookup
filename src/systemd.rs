use zbus_systemd::timedate1::TimedatedProxy;
use zbus::Connection;

#[derive(Debug, thiserror::Error)]
pub enum SetTimezoneError {
    #[error("Failed to connect to system D-Bus")]
    Connection(#[source] zbus::Error),

    #[error("Failed to create timedated proxy")]
    Proxy(#[source] zbus::Error),

    #[error("Failed to set timezone to '{timezone}'")]
    SetTimezone {
        timezone: String,
        #[source]
        source: zbus::Error,
    },
}

pub async fn set_tz(tz: &str) -> Result<(), SetTimezoneError> {
    let connection = Connection::system()
        .await
        .map_err(SetTimezoneError::Connection)?;

    let proxy = TimedatedProxy::new(&connection)
        .await
        .map_err(SetTimezoneError::Proxy)?;

    proxy
        .set_timezone(tz.to_string(), true)
        .await
        .map_err(|source| SetTimezoneError::SetTimezone {
            timezone: tz.to_string(),
            source,
        })?;

    Ok(())
}

