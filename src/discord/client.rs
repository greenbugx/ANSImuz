use presenceforge::activity::Activity;
use presenceforge::error::DiscordIpcError;
use presenceforge::sync::DiscordIpcClient;
use std::fmt;

// ANSImuz discord application publicly accessible client id
pub const DEFAULT_CLIENT_ID: &str = "1569455150546817146";

#[derive(Debug)]
pub enum DiscordError {
    Ipc(DiscordIpcError),
    NotConnected,
}

impl fmt::Display for DiscordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ipc(e) => write!(f, "Discord IPC error: {e}"),
            Self::NotConnected => write!(f, "Discord client is not connected"),
        }
    }
}

impl std::error::Error for DiscordError {}

impl From<DiscordIpcError> for DiscordError {
    fn from(err: DiscordIpcError) -> Self {
        Self::Ipc(err)
    }
}

pub struct DiscordClient {
    client_id: String,
    inner: Option<DiscordIpcClient>,
}

impl DiscordClient {
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            inner: None,
        }
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn is_connected(&self) -> bool {
        self.inner.as_ref().is_some_and(|c| c.is_connected())
    }

    pub fn connect(&mut self) -> Result<(), DiscordError> {
        let mut client = DiscordIpcClient::new(&self.client_id)?;
        client.connect()?;
        self.inner = Some(client);
        Ok(())
    }

    pub fn set_activity(&mut self, activity: &Activity) -> Result<(), DiscordError> {
        let client = self.inner.as_mut().ok_or(DiscordError::NotConnected)?;
        client.set_activity(activity)?;
        Ok(())
    }

    pub fn clear_activity(&mut self) -> Result<(), DiscordError> {
        if let Some(client) = self.inner.as_mut()
            && client.is_connected()
        {
            client.clear_activity()?;
        }
        Ok(())
    }

    pub fn disconnect(&mut self) {
        if let Some(mut client) = self.inner.take() {
            if client.is_connected() {
                let _ = client.clear_activity();
            }
            client.close();
        }
    }
}

impl Drop for DiscordClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use presenceforge::ActivityBuilder;

    #[test]
    fn test_client_offline_contract_and_safe_lifecycle() {
        let mut client = DiscordClient::new("1569455150546817146");
        assert_eq!(client.client_id(), "1569455150546817146");
        assert!(!client.is_connected());

        // Attempting to set activity when disconnected must fail with NotConnected
        let activity = ActivityBuilder::new().details("Track").build();
        assert!(matches!(
            client.set_activity(&activity),
            Err(DiscordError::NotConnected)
        ));

        // clear_activity when not connected must safely return Ok(()) without error
        assert!(client.clear_activity().is_ok());

        // disconnect when not connected must be safe and idempotent
        client.disconnect();
        client.disconnect();
        assert!(!client.is_connected());
    }

    #[test]
    fn test_discord_error_formatting_and_traits() {
        let err_not_connected = DiscordError::NotConnected;
        assert_eq!(
            format!("{err_not_connected}"),
            "Discord client is not connected"
        );

        let ipc_err = DiscordIpcError::ConnectionFailed(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "pipe not found",
        ));
        let err_from_ipc: DiscordError = ipc_err.into();
        assert!(format!("{err_from_ipc}").contains("Discord IPC error:"));

        // Verify std::error::Error implementation
        let boxed: Box<dyn std::error::Error> = Box::new(err_not_connected);
        assert_eq!(boxed.to_string(), "Discord client is not connected");
    }
}
