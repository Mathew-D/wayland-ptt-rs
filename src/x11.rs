use std::collections::VecDeque;
use std::fmt;

use x11rb::connection::{Connection, RequestConnection};
use x11rb::errors::{ConnectError, ConnectionError, ReplyError};
use x11rb::protocol::xinput::{
    self, ConnectionExt as _, DeviceUse, X11_EXTENSION_NAME as XINPUT_EXTENSION_NAME,
};
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, ButtonPressEvent, EventMask,
    KEY_PRESS_EVENT, KEY_RELEASE_EVENT, KeyButMask, KeyPressEvent, Window,
};
use x11rb::protocol::xtest::{ConnectionExt as _, X11_EXTENSION_NAME};
use x11rb::rust_connection::RustConnection;
use xkbcommon_rs::keysym::keysym_from_name;

use crate::args::Config;
use crate::uinput::{UinputError, UinputKeyboard};

const XTEST_MAJOR_VERSION: u8 = 2;
const XTEST_MINOR_VERSION: u16 = 2;
const XINPUT_DEVICE_KEY_PRESS: u8 = 1;
const XINPUT_DEVICE_KEY_RELEASE: u8 = 2;
const XINPUT_DEVICE_BUTTON_PRESS: u8 = 3;
const XINPUT_DEVICE_BUTTON_RELEASE: u8 = 4;

#[derive(Debug, Copy, Clone)]
struct XTestInput {
    event_base: u8,
    keyboard_device: Option<u8>,
    pointer_device: Option<u8>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum InputState {
    Pressed,
    Released,
}

pub struct X11Config {
    pub connection: RustConnection,
    pub screen_num: usize,
    target_window: Option<Window>,
    pressed_window: Option<Window>,
    wm_class: Atom,
    net_wm_name: Atom,
    wm_name: Atom,
    xtest: bool,
    xtest_input: Option<XTestInput>,
    uinput: Option<UinputKeyboard>,
}

impl X11Config {
    pub fn set_uinput(&mut self, keyboard: UinputKeyboard) {
        self.uinput = Some(keyboard);
    }
}

#[derive(Debug)]
pub enum ConfigureX11Error {
    Connect {
        display_name: Option<String>,
        source: ConnectError,
    },
    MissingXtestExtension,
    QueryXtestVersion {
        source: ReplyError,
    },
    QueryXinput {
        source: ReplyError,
    },
    Uinput {
        source: UinputError,
    },
    InvalidSendKey {
        key: String,
    },
    UnmappedSendKey {
        key: String,
    },
    InvalidMouseButton {
        button: u32,
    },
    QueryKeyboardMapping {
        source: ReplyError,
    },
    SendInput {
        source: ReplyError,
    },
    FindWindow {
        source: ReplyError,
    },
}

impl fmt::Display for ConfigureX11Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect {
                display_name,
                source,
            } => match display_name {
                Some(display_name) => write!(
                    f,
                    "Failed to connect to X11 display {display_name}: {source}"
                ),
                None => write!(f, "Failed to connect to the default X11 display: {source}"),
            },
            Self::MissingXtestExtension => {
                write!(f, "X11 server does not support the XTEST extension")
            }
            Self::QueryXtestVersion { source } => {
                write!(f, "Failed to query XTEST version: {source}")
            }
            Self::QueryXinput { source } => {
                write!(f, "Failed to query XInput devices: {source}")
            }
            Self::Uinput { source } => write!(f, "{source}"),
            Self::InvalidSendKey { key } => {
                write!(f, "Failed to resolve X11 send key: {key}")
            }
            Self::UnmappedSendKey { key } => {
                write!(f, "X11 server has no keycode mapped for send key: {key}")
            }
            Self::InvalidMouseButton { button } => {
                write!(f, "Invalid X11 mouse button: {button}")
            }
            Self::QueryKeyboardMapping { source } => {
                write!(f, "Failed to query X11 keyboard mapping: {source}")
            }
            Self::SendInput { source } => {
                write!(f, "Failed to send X11 input event: {source}")
            }
            Self::FindWindow { source } => write!(f, "Failed to find target window: {source}"),
        }
    }
}

impl std::error::Error for ConfigureX11Error {}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum X11Target {
    Key { keycode: u8 },
    MouseButton { button: u8 },
}

pub fn configure_x11() -> Result<X11Config, ConfigureX11Error> {
    configure_x11_with_display_name_and_backend(None, false)
}

pub fn configure_x11_with_display_name(
    display_name: Option<&str>,
) -> Result<X11Config, ConfigureX11Error> {
    configure_x11_with_display_name_and_backend(display_name, false)
}

pub fn configure_x11_with_backend(xtest: bool) -> Result<X11Config, ConfigureX11Error> {
    configure_x11_with_display_name_and_backend(None, xtest)
}

fn configure_x11_with_display_name_and_backend(
    display_name: Option<&str>,
    xtest: bool,
) -> Result<X11Config, ConfigureX11Error> {
    let (connection, screen_num) =
        x11rb::connect(display_name).map_err(|source| ConfigureX11Error::Connect {
            display_name: display_name.map(ToOwned::to_owned),
            source,
        })?;

    if xtest {
        ensure_xtest_extension(&connection)?;
        connection
            .xtest_get_version(XTEST_MAJOR_VERSION, XTEST_MINOR_VERSION)
            .map_err(connection_error_to_configure_x11_error)?
            .reply()
            .map_err(|source| ConfigureX11Error::QueryXtestVersion { source })?;
    }
    let xtest_input = if xtest {
        query_xtest_input(&connection)?
    } else {
        None
    };

    let atom = |name: &[u8]| -> Result<Atom, ConfigureX11Error> {
        Ok(connection
            .intern_atom(false, name)
            .map_err(|source| ConfigureX11Error::FindWindow {
                source: ReplyError::ConnectionError(source),
            })?
            .reply()
            .map_err(|source| ConfigureX11Error::FindWindow { source })?
            .atom)
    };
    let wm_class = atom(b"WM_CLASS")?;
    let net_wm_name = atom(b"_NET_WM_NAME")?;
    let wm_name = atom(b"WM_NAME")?;

    Ok(X11Config {
        connection,
        screen_num,
        target_window: None,
        pressed_window: None,
        wm_class,
        net_wm_name,
        wm_name,
        xtest,
        xtest_input,
        uinput: None,
    })
}

pub fn configure_x11_target(
    x11_config: &X11Config,
    config: &Config,
) -> Result<X11Target, ConfigureX11Error> {
    if let Some(button) = config.mouse_button {
        let button =
            u8::try_from(button).map_err(|_| ConfigureX11Error::InvalidMouseButton { button })?;
        return Ok(X11Target::MouseButton { button });
    }

    let keysym = resolve_send_keysym(&config.send_key)?;
    let keycode = find_keycode_for_keysym(x11_config, keysym, &config.send_key)?;

    Ok(X11Target::Key { keycode })
}

pub fn send_target_state(
    x11_config: &mut X11Config,
    target: X11Target,
    state: InputState,
    verbose: bool,
    target_name: &str,
) -> Result<(), ConfigureX11Error> {
    let (event_type, detail) = event_type_and_detail(target, state);

    if !x11_config.xtest {
        return send_direct(x11_config, target, state, verbose, target_name);
    }

    if let X11Target::Key { .. } = target {
        if let Some(keyboard) = x11_config.uinput.as_mut() {
            if verbose {
                eprintln!("uinput {target:?} {state:?} code={}", keyboard.code());
            }
            keyboard
                .send(state == InputState::Pressed)
                .map_err(|source| ConfigureX11Error::Uinput { source })?;
            return Ok(());
        }
    }

    let (type_, deviceid) = x11_config
        .xtest_input
        .and_then(|input| xtest_device_event(&input, target, state))
        .unwrap_or((event_type, 0));
    if verbose {
        eprintln!("XTEST {target:?} {state:?} type={type_} detail={detail} device={deviceid}");
    }
    let cookie = x11_config
        .connection
        .xtest_fake_input(
            type_,
            detail,
            x11rb::CURRENT_TIME,
            x11rb::NONE,
            0,
            0,
            deviceid,
        )
        .map_err(connection_error_to_send_input_error)?;
    cookie
        .check()
        .map_err(|source| ConfigureX11Error::SendInput { source })?;
    x11_config
        .connection
        .flush()
        .map_err(connection_error_to_send_input_error)?;

    Ok(())
}

fn property_text(connection: &RustConnection, window: Window, atom: Atom) -> Option<String> {
    let reply = connection
        .get_property(false, window, atom, AtomEnum::ANY, 0, 256)
        .ok()?
        .reply()
        .ok()?;
    if reply.format != 8 {
        return None;
    }
    Some(String::from_utf8_lossy(&reply.value).into_owned())
}

fn target_match_score(config: &X11Config, window: Window, target: &str) -> u8 {
    let target = target.to_ascii_lowercase();
    if let Some(class) = property_text(&config.connection, window, config.wm_class) {
        if class
            .split('\0')
            .any(|part| part.to_ascii_lowercase().contains(&target))
        {
            return 2;
        }
        // A browser tab may have the target name in its title. Its WM_CLASS belongs to the browser.
        if !class.is_empty() {
            return 0;
        }
    }
    for atom in [config.net_wm_name, config.wm_name] {
        if property_text(&config.connection, window, atom)
            .is_some_and(|name| name.to_ascii_lowercase().contains(&target))
        {
            return 1;
        }
    }
    0
}

fn find_target_window(
    config: &X11Config,
    target: &str,
) -> Result<Option<Window>, ConfigureX11Error> {
    let root = config.connection.setup().roots[config.screen_num].root;
    let mut pending = VecDeque::from([root]);
    let mut title_match = None;
    while let Some(window) = pending.pop_front() {
        match target_match_score(config, window, target) {
            2 => return Ok(Some(window)),
            1 if title_match.is_none() => title_match = Some(window),
            _ => {}
        }
        // Windows may disappear while the tree is being traversed.
        if let Ok(cookie) = config.connection.query_tree(window) {
            if let Ok(tree) = cookie.reply() {
                pending.extend(tree.children);
            }
        }
    }
    Ok(title_match)
}

fn window_exists(config: &X11Config, window: Window) -> bool {
    config
        .connection
        .get_window_attributes(window)
        .ok()
        .is_some_and(|cookie| cookie.reply().is_ok())
}

fn send_direct(
    config: &mut X11Config,
    target: X11Target,
    state: InputState,
    verbose: bool,
    target_name: &str,
) -> Result<(), ConfigureX11Error> {
    if state == InputState::Pressed && config.pressed_window.is_some() {
        return Ok(());
    }
    if state == InputState::Released && config.pressed_window.is_none() {
        return Ok(());
    }
    if let Some(pressed_window) = config.pressed_window {
        if !window_exists(config, pressed_window) {
            config.pressed_window = None;
            config.target_window = None;
            if verbose {
                eprintln!("Target window disappeared before release; event skipped");
            }
            return Ok(());
        }
    }
    if config
        .target_window
        .is_some_and(|window| !window_exists(config, window))
    {
        config.target_window = None;
    }
    if config.target_window.is_none() {
        config.target_window = find_target_window(config, target_name)?;
        if verbose {
            match config.target_window {
                Some(window) => eprintln!("Detected {target_name} window: {window:#x}"),
                None => eprintln!("{target_name} window not found; event skipped"),
            }
        }
    }
    let Some(window) = config.pressed_window.or(config.target_window) else {
        return Ok(());
    };
    let root = config.connection.setup().roots[config.screen_num].root;
    let (event_type, detail) = event_type_and_detail(target, state);
    let event: [u8; 32] = match target {
        X11Target::Key { .. } => KeyPressEvent {
            response_type: event_type,
            detail,
            sequence: 0,
            time: x11rb::CURRENT_TIME,
            root,
            event: window,
            child: x11rb::NONE,
            root_x: 0,
            root_y: 0,
            event_x: 0,
            event_y: 0,
            state: KeyButMask::default(),
            same_screen: true,
        }
        .into(),
        X11Target::MouseButton { .. } => ButtonPressEvent {
            response_type: event_type,
            detail,
            sequence: 0,
            time: x11rb::CURRENT_TIME,
            root,
            event: window,
            child: x11rb::NONE,
            root_x: 0,
            root_y: 0,
            event_x: 0,
            event_y: 0,
            state: KeyButMask::default(),
            same_screen: true,
        }
        .into(),
    };
    if verbose {
        eprintln!("SendEvent {target:?} {state:?} to {target_name} window {window:#x}");
    }
    // NO_EVENT delivers to the client that owns this window, even if it did not
    // select KeyPress/ButtonPress on this exact window.
    let result = config
        .connection
        .send_event(false, window, EventMask::NO_EVENT, event)
        .map_err(connection_error_to_send_input_error)?
        .check()
        .map_err(|source| ConfigureX11Error::SendInput { source });
    if let Err(err) = result {
        config.target_window = None;
        config.pressed_window = None;
        if verbose {
            eprintln!("SendEvent failed: {err}");
        }
        return Err(err);
    }
    config
        .connection
        .flush()
        .map_err(connection_error_to_send_input_error)?;
    config.pressed_window = if state == InputState::Pressed {
        Some(window)
    } else {
        None
    };
    if verbose {
        eprintln!("SendEvent request completed ({target_name} delivery and handling unverified)");
    }
    Ok(())
}

fn ensure_xtest_extension(connection: &RustConnection) -> Result<(), ConfigureX11Error> {
    let extension = connection
        .extension_information(X11_EXTENSION_NAME)
        .map_err(connection_error_to_configure_x11_error)?;

    if extension.is_some() {
        Ok(())
    } else {
        Err(ConfigureX11Error::MissingXtestExtension)
    }
}

fn query_xtest_input(connection: &RustConnection) -> Result<Option<XTestInput>, ConfigureX11Error> {
    let Some(info) = connection
        .extension_information(XINPUT_EXTENSION_NAME)
        .map_err(|source| ConfigureX11Error::QueryXinput {
            source: ReplyError::ConnectionError(source),
        })?
    else {
        return Ok(None);
    };
    let reply = connection
        .xinput_list_input_devices()
        .map_err(|source| ConfigureX11Error::QueryXinput {
            source: ReplyError::ConnectionError(source),
        })?
        .reply()
        .map_err(|source| ConfigureX11Error::QueryXinput { source })?;

    Ok(Some(XTestInput {
        event_base: info.first_event,
        keyboard_device: xtest_device_id(&reply, DeviceUse::IS_X_EXTENSION_KEYBOARD),
        pointer_device: xtest_device_id(&reply, DeviceUse::IS_X_EXTENSION_POINTER),
    }))
}

fn xtest_device_id(reply: &xinput::ListInputDevicesReply, device_use: DeviceUse) -> Option<u8> {
    reply
        .devices
        .iter()
        .zip(reply.names.iter())
        .find_map(|(device, name)| {
            if device.device_use != device_use {
                return None;
            }
            let name = String::from_utf8_lossy(&name.name);
            name.to_ascii_lowercase()
                .contains("xtest")
                .then_some(device.device_id)
        })
}

fn xtest_device_event(
    input: &XTestInput,
    target: X11Target,
    state: InputState,
) -> Option<(u8, u8)> {
    let (event, device) = match (target, state) {
        (X11Target::Key { .. }, InputState::Pressed) => {
            (XINPUT_DEVICE_KEY_PRESS, input.keyboard_device)
        }
        (X11Target::Key { .. }, InputState::Released) => {
            (XINPUT_DEVICE_KEY_RELEASE, input.keyboard_device)
        }
        (X11Target::MouseButton { .. }, InputState::Pressed) => {
            (XINPUT_DEVICE_BUTTON_PRESS, input.pointer_device)
        }
        (X11Target::MouseButton { .. }, InputState::Released) => {
            (XINPUT_DEVICE_BUTTON_RELEASE, input.pointer_device)
        }
    };
    let device = device?;
    let event_type = input.event_base.checked_add(event)?;
    Some((event_type, device))
}

fn connection_error_to_configure_x11_error(source: ConnectionError) -> ConfigureX11Error {
    match source {
        ConnectionError::UnsupportedExtension => ConfigureX11Error::MissingXtestExtension,
        source => ConfigureX11Error::QueryXtestVersion {
            source: ReplyError::ConnectionError(source),
        },
    }
}

fn connection_error_to_send_input_error(source: ConnectionError) -> ConfigureX11Error {
    ConfigureX11Error::SendInput {
        source: ReplyError::ConnectionError(source),
    }
}

fn resolve_send_keysym(key: &str) -> Result<u32, ConfigureX11Error> {
    keysym_from_name(key, 0)
        .map(u32::from)
        .ok_or_else(|| ConfigureX11Error::InvalidSendKey {
            key: key.to_string(),
        })
}

fn find_keycode_for_keysym(
    x11_config: &X11Config,
    keysym: u32,
    key: &str,
) -> Result<u8, ConfigureX11Error> {
    let setup = x11_config.connection.setup();
    let first_keycode = setup.min_keycode;
    let keycode_count = setup.max_keycode - setup.min_keycode + 1;
    let mapping = x11_config
        .connection
        .get_keyboard_mapping(first_keycode, keycode_count)
        .map_err(|source| ConfigureX11Error::QueryKeyboardMapping {
            source: ReplyError::ConnectionError(source),
        })?
        .reply()
        .map_err(|source| ConfigureX11Error::QueryKeyboardMapping { source })?;

    let keysyms_per_keycode = usize::from(mapping.keysyms_per_keycode);
    for (index, keysyms) in mapping.keysyms.chunks(keysyms_per_keycode).enumerate() {
        if keysyms.contains(&keysym) {
            return Ok(first_keycode + index as u8);
        }
    }

    Err(ConfigureX11Error::UnmappedSendKey {
        key: key.to_string(),
    })
}

fn event_type_and_detail(target: X11Target, state: InputState) -> (u8, u8) {
    match (target, state) {
        (X11Target::Key { keycode }, InputState::Pressed) => (KEY_PRESS_EVENT, keycode),
        (X11Target::Key { keycode }, InputState::Released) => (KEY_RELEASE_EVENT, keycode),
        (X11Target::MouseButton { button }, InputState::Pressed) => (BUTTON_PRESS_EVENT, button),
        (X11Target::MouseButton { button }, InputState::Released) => (BUTTON_RELEASE_EVENT, button),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ConfigureX11Error, InputState, X11Target, XTestInput, event_type_and_detail,
        resolve_send_keysym, xtest_device_event,
    };
    use x11rb::protocol::xproto::{
        BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
    };

    #[test]
    fn resolves_valid_send_key_name() {
        // Uses the left SUPER key as a test key.
        let keysym = resolve_send_keysym("Super_L").unwrap();
        assert_ne!(keysym, 0);
    }

    #[test]
    fn rejects_invalid_send_key_name() {
        let err = resolve_send_keysym("NOT_A_REAL_X11_KEY").unwrap_err();

        match err {
            ConfigureX11Error::InvalidSendKey { key } => {
                assert_eq!(key, "NOT_A_REAL_X11_KEY")
            }
            other => panic!("expected InvalidSendKey, got {other:?}"),
        }
    }

    #[test]
    fn maps_key_press_event_type_and_detail() {
        assert_eq!(
            event_type_and_detail(X11Target::Key { keycode: 42 }, InputState::Pressed),
            (KEY_PRESS_EVENT, 42)
        );
    }

    #[test]
    fn maps_key_release_event_type_and_detail() {
        assert_eq!(
            event_type_and_detail(X11Target::Key { keycode: 42 }, InputState::Released),
            (KEY_RELEASE_EVENT, 42)
        );
    }

    #[test]
    fn maps_mouse_press_event_type_and_detail() {
        assert_eq!(
            event_type_and_detail(X11Target::MouseButton { button: 5 }, InputState::Pressed),
            (BUTTON_PRESS_EVENT, 5)
        );
    }

    #[test]
    fn maps_mouse_release_event_type_and_detail() {
        assert_eq!(
            event_type_and_detail(X11Target::MouseButton { button: 5 }, InputState::Released),
            (BUTTON_RELEASE_EVENT, 5)
        );
    }

    #[test]
    fn maps_xtest_device_events() {
        let input = XTestInput {
            event_base: 66,
            keyboard_device: Some(5),
            pointer_device: Some(4),
        };

        assert_eq!(
            xtest_device_event(&input, X11Target::Key { keycode: 49 }, InputState::Pressed),
            Some((67, 5))
        );
        assert_eq!(
            xtest_device_event(&input, X11Target::Key { keycode: 49 }, InputState::Released),
            Some((68, 5))
        );
        assert_eq!(
            xtest_device_event(
                &input,
                X11Target::MouseButton { button: 1 },
                InputState::Pressed
            ),
            Some((69, 4))
        );
        assert_eq!(
            xtest_device_event(
                &input,
                X11Target::MouseButton { button: 1 },
                InputState::Released
            ),
            Some((70, 4))
        );
    }

    #[test]
    fn xtest_device_event_requires_matching_device() {
        let input = XTestInput {
            event_base: 66,
            keyboard_device: Some(5),
            pointer_device: None,
        };

        assert_eq!(
            xtest_device_event(&input, X11Target::Key { keycode: 49 }, InputState::Pressed),
            Some((67, 5))
        );
        assert_eq!(
            xtest_device_event(
                &input,
                X11Target::MouseButton { button: 1 },
                InputState::Pressed
            ),
            None
        );
    }
}
