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
