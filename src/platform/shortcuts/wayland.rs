//! Global shortcuts on Wayland.
//!
//! Wayland forbids a client grabbing keys directly, so shortcuts go through the
//! `org.freedesktop.portal.GlobalShortcuts` portal. The desktop owns the binding
//! and tells us when it fires; the user can also change it in their own settings.
//!
//! If the portal or its backend is missing, [`bind`] reports why, so the UI can say
//! shortcuts are unavailable instead of silently not working.

use std::sync::Arc;
use std::sync::Mutex;

use crate::platform::shortcuts::Action;
use futures_util::StreamExt;

/// The portal's well-known names.
const PORTAL_BUS: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const SHORTCUTS_INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
const REQUEST_INTERFACE: &str = "org.freedesktop.portal.Request";
const SESSION_INTERFACE: &str = "org.freedesktop.portal.Session";

/// Errors from the portal.
#[derive(Debug, thiserror::Error)]
pub enum PortalError {
    #[error("the desktop portal is not reachable: {0}")]
    Bus(String),
    #[error("this desktop does not implement global shortcuts through the portal")]
    Unsupported,
    #[error("the portal refused the request: {0}")]
    Refused(String),
    #[error(
        "this system's portal cannot resolve application ids, so global shortcuts cannot be \
         bound. Shortcuts still work under X11 or XWayland."
    )]
    RegistryUnavailable,
}

/// The preferred keys for each action, in portal notation.
pub fn accelerator(action: Action) -> &'static str {
    match action {
        Action::ToggleDashboard => "CTRL+SHIFT+d",
        Action::ToggleWindow => "CTRL+SHIFT+backslash",
        Action::FocusInput => "CTRL+SHIFT+i",
        Action::SystemAudio => "CTRL+SHIFT+m",
        Action::VoiceInput => "CTRL+SHIFT+a",
        Action::Screenshot => "CTRL+SHIFT+s",
        Action::PrivacyMode | Action::NormalMode => "",
    }
}

/// Shortcut description shown in the portal's own configuration UI.
pub fn description(action: Action) -> &'static str {
    action.label()
}

/// Bound shortcuts, with a queue of activations.
pub struct PortalShortcuts {
    actions: Arc<Mutex<Vec<String>>>,
    session: zbus::Connection,
    session_handle: String,
}

impl PortalShortcuts {
    /// Bind the default shortcuts and start listening.
    ///
    /// Returns a stream of actions through [`Self::drain`].
    pub async fn bind(actions: &[Action]) -> Result<Self, PortalError> {
        let connection = zbus::Connection::session()
            .await
            .map_err(|error| PortalError::Bus(error.to_string()))?;

        // The portal resolves the app id through a desktop entry; install ours
        // before registering so an unpackaged binary still works.
        if let Err(error) = ensure_desktop_entry() {
            tracing::warn!(%error, "could not install the desktop entry");
        }

        // An unsandboxed binary has no app id, so the portal refuses every call
        // until the connection is registered. This must happen first.
        register_app_id(&connection).await?;

        let proxy = zbus::Proxy::new(&connection, PORTAL_BUS, PORTAL_PATH, SHORTCUTS_INTERFACE)
            .await
            .map_err(|error| PortalError::Bus(error.to_string()))?;

        // A missing interface means the desktop has no global-shortcut portal.
        let version: Result<u32, _> = proxy.get_property("version").await;
        if version.is_err() {
            return Err(PortalError::Unsupported);
        }

        // 1. Create a session.
        let mut options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>> =
            std::collections::HashMap::new();
        // The portal requires an application id; without one it answers
        // NotAllowed. Set it explicitly for an un-packaged binary.
        options.insert(
            "handle_token",
            zbus::zvariant::Value::from(format!("lennette{}", std::process::id())),
        );
        options.insert(
            "session_handle_token",
            zbus::zvariant::Value::from(format!("session{}", std::process::id())),
        );
        options.insert(
            "handle_token",
            zbus::zvariant::Value::from(format!("lennette{}", std::process::id())),
        );
        options.insert(
            "session_handle_token",
            zbus::zvariant::Value::from(format!("session{}", std::process::id())),
        );
        let reply = proxy
            .call_method("CreateSession", &(options))
            .await
            .map_err(|error| PortalError::Refused(error.to_string()))?;
        let request_path: zbus::zvariant::OwnedObjectPath = reply
            .body()
            .deserialize()
            .map_err(|error| PortalError::Refused(error.to_string()))?;

        // 2. Wait for the request to yield a session handle.
        let request = zbus::Proxy::new(
            &connection,
            PORTAL_BUS,
            request_path.as_str(),
            REQUEST_INTERFACE,
        )
        .await
        .map_err(|error| PortalError::Bus(error.to_string()))?;
        let session_path = wait_for_request(&request)
            .await
            .ok_or_else(|| PortalError::Refused("the portal did not create a session".into()))?;

        // 3. Bind our shortcuts on that session.
        let mut shortcut_list: Vec<(
            String,
            std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
        )> = Vec::new();
        for action in actions {
            let mut props: std::collections::HashMap<&str, zbus::zvariant::Value<'_>> =
                std::collections::HashMap::new();
            props.insert(
                "description",
                zbus::zvariant::Value::from(description(*action)),
            );
            props.insert(
                "preferred_trigger",
                zbus::zvariant::Value::from(accelerator(*action)),
            );
            shortcut_list.push((action.id().to_owned(), props));
        }
        let empty: std::collections::HashMap<&str, zbus::zvariant::Value<'_>> =
            std::collections::HashMap::new();
        let bind_reply = proxy
            .call_method(
                "BindShortcuts",
                &(
                    zbus::zvariant::ObjectPath::try_from(session_path.as_str())
                        .map_err(|error| PortalError::Refused(error.to_string()))?,
                    shortcut_list,
                    "",
                    empty,
                ),
            )
            .await
            .map_err(|error| PortalError::Refused(error.to_string()))?;
        let bind_path: zbus::zvariant::OwnedObjectPath = bind_reply
            .body()
            .deserialize()
            .map_err(|error| PortalError::Refused(error.to_string()))?;
        let bind_request = zbus::Proxy::new(
            &connection,
            PORTAL_BUS,
            bind_path.as_str(),
            REQUEST_INTERFACE,
        )
        .await
        .map_err(|error| PortalError::Bus(error.to_string()))?;
        wait_for_request(&bind_request)
            .await
            .ok_or_else(|| PortalError::Refused("binding was refused".into()))?;

        // 4. Listen for activations.
        let actions_state = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = actions_state.clone();
        let mut signals = proxy
            .receive_signal("Activated")
            .await
            .map_err(|error| PortalError::Bus(error.to_string()))?;
        tokio::spawn(async move {
            while let Some(message) = signals.next().await {
                // Signature is (o session, s shortcut_id, a{sv} options).
                let id: Result<String, _> = message
                    .body()
                    .deserialize::<(
                        String,
                        String,
                        std::collections::HashMap<String, zbus::zvariant::Value<'_>>,
                    )>()
                    .map(|(_, id, _)| id);
                if let Ok(id) = id
                    && let Ok(mut queue) = sink.lock()
                {
                    queue.push(id);
                }
            }
        });

        Ok(Self {
            actions: actions_state,
            session: connection,
            session_handle: session_path,
        })
    }

    /// Take every action fired since the last call.
    pub fn drain(&self) -> Vec<Action> {
        self.actions
            .lock()
            .map(|mut queue| {
                std::mem::take(&mut *queue)
                    .iter()
                    .filter_map(|id| Action::from_id(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Close the portal session.
    pub async fn close(&self) {
        if let Ok(session) = zbus::Proxy::new(
            &self.session,
            PORTAL_BUS,
            self.session_handle.as_str(),
            SESSION_INTERFACE,
        )
        .await
        {
            let _ = session.call_method("Close", &()).await;
        }
    }
}

/// The application id used for portal calls.
const APP_ID: &str = "dev.lennette.overlay";

/// The desktop entry the portal needs to resolve the app id.
const DESKTOP_FILE: &str = r#"[Desktop Entry]
Type=Application
Name=lennette
GenericName=Ambient Real-Time Meeting Copilot
Comment=An ambient real-time meeting and discussion copilot
Exec=lennette
Icon=dev.lennette.overlay
Terminal=false
Categories=Utility;AudioVideo;Audio;Office;
Keywords=ai;meeting;transcription;copilot;speech;audio;notes;live;intelligence;
StartupWMClass=dev.lennette.overlay
"#;

/// Install the desktop entry into the user's applications directory.
///
/// The portal resolves an app id through a `.desktop` file, so an unpackaged
/// binary has to provide one before shortcuts can bind.
pub fn ensure_desktop_entry() -> Result<(), String> {
    let Some(dirs) = directories::BaseDirs::new() else {
        return Err("no home directory".to_owned());
    };
    let directory = dirs.data_dir().join("applications");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!("{APP_ID}.desktop"));
    if path.exists() {
        return Ok(());
    }
    std::fs::write(&path, DESKTOP_FILE).map_err(|error| error.to_string())?;
    // Refresh the desktop database so the portal notices it immediately.
    let _ = std::process::Command::new("update-desktop-database")
        .arg(&directory)
        .status();
    Ok(())
}

/// Associate this connection with an app id.
///
/// Sandboxed apps already have one; a plain binary must register. Registering
/// twice is an error, so a refusal because it is already registered is treated
/// as success.
async fn register_app_id(connection: &zbus::Connection) -> Result<(), PortalError> {
    let registry = zbus::Proxy::new(
        connection,
        PORTAL_BUS,
        // The Registry interface is exported on the portal object itself.
        PORTAL_PATH,
        "org.freedesktop.host.portal.Registry",
    )
    .await
    .map_err(|error| PortalError::Bus(error.to_string()))?;

    let options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>> =
        std::collections::HashMap::new();
    match registry.call_method("Register", &(APP_ID, options)).await {
        Ok(_) => Ok(()),
        Err(error) => {
            let text = error.to_string();
            // "already registered" is fine: the id is what we want either way.
            if text.contains("already") || text.contains("Exists") {
                return Ok(());
            }
            // Some systems cannot resolve any app id (observed on NixOS, where the
            // portal cannot see desktop files). That is an environment limit, and
            // is reported as such rather than as a generic refusal.
            if text.contains("App info not found") {
                return Err(PortalError::RegistryUnavailable);
            }
            Err(PortalError::Refused(text))
        }
    }
}

/// Poll a request proxy for its response, returning the session or handle.
async fn wait_for_request(request: &zbus::Proxy<'_>) -> Option<String> {
    let mut signals = request.receive_signal("Response").await.ok()?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let next = tokio::time::timeout_at(deadline, signals.next())
            .await
            .ok()??;
        // Response is (u response, a{sv} results).
        // The body must outlive the deserialize result.
        let body = next.body();
        let parsed: Result<
            (
                u32,
                std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
            ),
            _,
        > = body.deserialize();
        if let Ok((code, results)) = parsed {
            if code != 0 {
                return None;
            }
            for key in ["session_handle", "shortcuts"] {
                if let Some(value) = results.get(key)
                    && let Ok(path) = zbus::zvariant::OwnedObjectPath::try_from(value.clone())
                {
                    return Some(path.as_str().to_owned());
                }
            }
            // A successful request with no handle still counts as success.
            return Some(String::new());
        }
    }
}

/// Whether the portal is usable in this session.
pub async fn available() -> bool {
    let Ok(connection) = zbus::Connection::session().await else {
        return false;
    };
    let Ok(proxy) =
        zbus::Proxy::new(&connection, PORTAL_BUS, PORTAL_PATH, SHORTCUTS_INTERFACE).await
    else {
        return false;
    };
    proxy.get_property::<u32>("version").await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_an_accelerator_and_description() {
        for action in Action::ALL {
            assert!(!accelerator(action).is_empty());
            assert!(accelerator(action).contains('+'));
            assert!(!description(action).is_empty());
        }
    }

    #[test]
    fn registry_failure_is_described_without_jargon() {
        let message = PortalError::RegistryUnavailable.to_string();
        assert!(message.contains("cannot be bound"));
        assert!(message.contains("X11"));
    }

    #[test]
    fn desktop_entry_matches_the_app_id() {
        assert!(DESKTOP_FILE.contains("Name=lennette"));
        assert!(DESKTOP_FILE.contains("Type=Application"));
        assert!(APP_ID.starts_with("dev.lennette"));
    }

    #[test]
    fn accelerators_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for action in Action::ALL {
            assert!(seen.insert(accelerator(action)), "duplicate for {action:?}");
        }
    }
}
