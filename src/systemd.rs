use zbus_systemd::timedate1::TimedatedProxy;
use zbus::Connection;
use tokio;
use anyhow::{Result};

pub fn set_tz(tz: &str) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let connection = Connection::system().await?;
        let proxy = TimedatedProxy::new(&connection).await?;
        proxy.set_timezone(tz.to_string(), true).await?;
        Ok(())
    })
}

