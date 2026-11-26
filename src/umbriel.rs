use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};

use crate::args::Config;
use crate::x11::{InputState, X11Config, X11Target, send_target_state};

pub mod umbriel_global_shortcuts_v1 {
    use wayland_client;

    pub mod __interfaces {
        wayland_scanner::generate_interfaces!("protocols/umbriel-global-shortcuts-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/umbriel-global-shortcuts-v1.xml");
}

use umbriel_global_shortcuts_v1::{
    umbriel_global_shortcuts_manager_v1::UmbrielGlobalShortcutsManagerV1,
    umbriel_global_shortcuts_session_v1::{
        Event as ShortcutEvent, UmbrielGlobalShortcutsSessionV1,
    },
};

const MANAGER_INTERFACE: &str = "umbriel_global_shortcuts_manager_v1";
const RESTORE_REQUEST_ID: u32 = 1;
const BIND_REQUEST_ID: u32 = 2;

pub fn credential_path(config: &Config) -> PathBuf {
    let state_home = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("."));

    state_home.join("wayland-ptt").join(credential_file_name(
        &config.application,
        &config.shortcut_id,
    ))
}

fn credential_file_name(application: &str, shortcut_id: &str) -> String {
    let mut name = String::new();
    for byte in format!("{application}-{shortcut_id}").bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            name.push(byte as char);
        } else {
            name.push('_');
        }
    }
    if name.is_empty() {
        name.push_str("credential");
    }
    format!(
        "{name}-{:016x}",
        stable_hash(&format!("{application}\0{shortcut_id}"))
    )
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn read_credential(path: &Path) -> Option<String> {
    let credential = fs::read_to_string(path).ok()?;
    let credential = credential.trim();
    (!credential.is_empty()).then(|| credential.to_string())
}

fn save_credential(path: &Path, credential: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(format!(".tmp-{}", std::process::id()));
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, credential)?;
    set_private_permissions(&temporary)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

struct State {
    manager: Option<UmbrielGlobalShortcutsManagerV1>,
    session: Option<UmbrielGlobalShortcutsSessionV1>,
    credential_path: PathBuf,
    shortcut_id: String,
    verbose: bool,
    running: bool,
    bound: bool,
    active: bool,
    x11_config: X11Config,
    x11_target: X11Target,
    target_name: String,
    error: Option<String>,
}

impl State {
    fn fail(&mut self, message: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some(message.into());
        }
        self.running = false;
    }

    fn forward(&mut self, state: InputState) {
        if state == InputState::Pressed {
            if self.active {
                return;
            }
            self.active = true;
        } else if !self.active {
            return;
        } else {
            self.active = false;
        }

        if let Err(err) = send_target_state(
            &mut self.x11_config,
            self.x11_target,
            state,
            self.verbose,
            &self.target_name,
        ) {
            self.fail(err.to_string());
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            if interface == MANAGER_INTERFACE {
                state.manager = Some(registry.bind(name, version.min(1), qh, ()));
            }
        }
    }
}

delegate_noop!(State: ignore UmbrielGlobalShortcutsManagerV1);

impl Dispatch<UmbrielGlobalShortcutsSessionV1, ()> for State {
    fn event(
        state: &mut Self,
        _session: &UmbrielGlobalShortcutsSessionV1,
        event: ShortcutEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ShortcutEvent::Grant { credential, .. } => {
                if let Err(err) = save_credential(&state.credential_path, &credential) {
                    state.fail(format!("Failed to save Umbriel shortcut credential: {err}"));
                } else if state.verbose {
                    eprintln!("Saved Umbriel shortcut credential");
                }
            }
            ShortcutEvent::Restored { status, .. } => {
                if state.verbose {
                    eprintln!("Umbriel credential restore status: {status}");
                }
            }
            ShortcutEvent::Shortcut {
                id,
                trigger_description,
                ..
            } => {
                if state.verbose {
                    eprintln!("Bound shortcut {id}: {trigger_description}");
                }
            }
            ShortcutEvent::Bound { status, .. } => {
                if status == 0 {
                    state.bound = true;
                    if state.verbose {
                        eprintln!("Umbriel shortcut bound");
                    }
                } else {
                    let reason = match status {
                        1 => "cancelled",
                        2 => "failed",
                        _ => "unknown",
                    };
                    state.fail(format!("Umbriel shortcut bind {reason}"));
                }
            }
            ShortcutEvent::Activated { id, .. } => {
                if id == state.shortcut_id {
                    if state.verbose {
                        eprintln!("Umbriel shortcut activated");
                    }
                    state.forward(InputState::Pressed);
                }
            }
            ShortcutEvent::Deactivated { id, .. } => {
                if id == state.shortcut_id {
                    if state.verbose {
                        eprintln!("Umbriel shortcut deactivated");
                    }
                    state.forward(InputState::Released);
                }
            }
            ShortcutEvent::Changed {
                id,
                trigger_description,
                ..
            } => {
                if state.verbose {
                    eprintln!("Umbriel shortcut {id} changed: {trigger_description}");
                }
            }
            ShortcutEvent::Listed { status, .. } => {
                if state.verbose {
                    eprintln!("Umbriel shortcut list status: {status}");
                }
            }
            ShortcutEvent::Closed => {
                state.forward(InputState::Released);
                if state.bound {
                    state.running = false;
                } else {
                    state.fail("Umbriel shortcut session closed before binding");
                }
            }
            _ => {}
        }
    }
}

pub fn run(config: &Config, x11_config: X11Config, x11_target: X11Target) -> Result<(), String> {
    let connection = Connection::connect_to_env().map_err(|err| err.to_string())?;
    let mut event_queue = connection.new_event_queue();
    let qh = event_queue.handle();
    connection.display().get_registry(&qh, ());

    let mut state = State {
        manager: None,
        session: None,
        credential_path: credential_path(config),
        shortcut_id: config.shortcut_id.clone(),
        verbose: config.verbose,
        running: true,
        bound: false,
        active: false,
        x11_config,
        x11_target,
        target_name: config.target.clone(),
        error: None,
    };

    event_queue
        .roundtrip(&mut state)
        .map_err(|err| err.to_string())?;

    let manager = state
        .manager
        .as_ref()
        .ok_or_else(|| "Umbriel global shortcuts manager is not available".to_string())?;
    let session = manager.create_session(config.application.clone(), &qh, ());

    if let Some(credential) = read_credential(&state.credential_path) {
        session.restore(RESTORE_REQUEST_ID, credential);
    }
    session.add_shortcut(
        config.shortcut_id.clone(),
        "Push-to-talk".to_string(),
        config.trigger.clone(),
    );
    session.bind(BIND_REQUEST_ID);
    state.session = Some(session);

    event_queue
        .roundtrip(&mut state)
        .map_err(|err| err.to_string())?;

    if let Some(err) = state.error.take() {
        return Err(err);
    }

    while state.running && state.error.is_none() {
        if let Err(err) = event_queue.blocking_dispatch(&mut state) {
            return Err(err.to_string());
        }
    }

    if let Some(err) = state.error {
        Err(err)
    } else {
        Ok(())
    }
}
