# wayland-ptt-rs

Background push-to-talk forwarding for X11 apps running under XWayland on Umbriel.

The daemon registers a keyboard chord with Umbriel's private `umbriel_global_shortcuts_manager_v1` protocol. Umbriel delivers `activated` when the chord is pressed and `deactivated` when it is released, even while a native Wayland window has focus. While the chord is held, the daemon emits the configured key through a `/dev/uinput` virtual keyboard so applications that read evdev directly (such as Mumble's global-shortcut engine) see it. If `/dev/uinput` is unavailable it falls back to XTEST injection.

The default configuration targets Mumble's Whisper/Shout shortcut, which is bound to X keycode `49` (the grave/backtick key, evdev code `41`) in `mumble_settings.json`:

- Umbriel trigger: `CTRL+grave`
- key sent while held: `grave`
- target: `mumble`

## Usage

```
wayland-ptt [-v] [--send-event] [--trigger TRIGGER] [--shortcut-id ID] [--application APP] [-s send_key] [-t target] [--vesktop-action ACTION] [--no-vesktop]
```

Examples:

```
wayland-ptt
wayland-ptt --trigger CTRL+Space -s grave -t mumble
wayland-ptt -s MOUSE5 -t discord
```

`--trigger` uses Umbriel's shortcut grammar: uppercase `CTRL`, `ALT`, `SHIFT`, and `LOGO` modifiers joined with `+`, followed by an XKB keysym name such as `grave`, `Space`, or `F13`.

`-s` accepts an XKB keysym name or `MOUSE<n>`. For keys, the default backend creates a uinput virtual keyboard (requires write access to `/dev/uinput`) so evdev readers see the press/release; it falls back to XTEST when uinput is unavailable. `--send-event` selects the older direct `SendEvent` backend for X11 apps that accept synthetic window events.

For Mumble, enable its evdev global-shortcut engine (`"enable_evdev": true` under `"shortcuts"` in `mumble_settings.json`) and restart Mumble. Because the virtual keyboard is a real input device, the emitted key can also reach the focused Wayland window; bind Mumble to a non-printing key if that is a problem.

`-t` matches case-insensitively against X11 `WM_CLASS` and window titles when using `--send-event`. With uinput/XTEST, input is injected globally and the target is used only for direct delivery and verbose logging.

## Vesktop / Discord

On each chord press and release the daemon also writes `run-shortcut <action>:down|up` to `$XDG_RUNTIME_DIR/vesktop.sock`, so one physical chord can drive both Mumble and Vesktop without needing a second Umbriel keybind. The default action is `pushToMute`; override it with `--vesktop-action` (e.g. `pushToTalk`, `toggleMute`) or disable it with `--no-vesktop`. This only does something while Vesktop is running — stock Discord has no such socket.

## Umbriel approval and credentials

The first `bind` for a new application/shortcut ID opens Umbriel's physical approval panel. Press Enter on the final approval page to approve it. The compositor-issued credential is stored privately at:

```
$XDG_STATE_HOME/wayland-ptt/<application>-<shortcut-id>-<hash>
```

or `~/.local/state/wayland-ptt/...` when `XDG_STATE_HOME` is unset. Later runs restore that credential and rotate it automatically. To force fresh approval, remove the credential file or change `--application`/`--shortcut-id`.

Umbriel stores the approved trigger durably. Changing `--trigger` does not change an already-approved shortcut unless you also change the shortcut ID or remove the stored grant.

## Installation

```
cargo build --release
sudo make install
```

`wayland-ptt.desktop` starts the daemon at login. Edit its `Exec=` line if you need non-default options.

## Reference

- [X11 KeySym Codes](https://github.com/xkbcommon/libxkbcommon/blob/master/include/xkbcommon/xkbcommon-keysyms.h) (ignore leading `XKB_KEY_`)

## License

MIT
