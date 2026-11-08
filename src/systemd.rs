use zbus_systemd::timedate1::TimedatedProxy;
use zbus::Connection;
use anyhow::{Result};

pub async fn set_tz(tz: &str) -> Result<()> {
    let connection = Connection::system().await?;
    let proxy = TimedatedProxy::new(&connection).await?;
    proxy.set_timezone(tz.to_string(), true).await?;
    Ok(())
}

