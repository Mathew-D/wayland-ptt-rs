use wayland_ptt::args::{Config, parse_args_from, usage};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn config(send_key: &str, mouse_button: Option<u32>) -> Config {
    Config {
        xtest: true,
        verbose: false,
        trigger: "CTRL+grave".to_string(),
        shortcut_id: "push-to-talk".to_string(),
        application: "wayland-ptt".to_string(),
        send_key: send_key.to_string(),
        mouse_button,
        target: "mumble".to_string(),
        vesktop_action: Some("pushToMute".to_string()),
    }
}

#[test]
fn parses_defaults_for_mumble_whisper() {
    let parsed = parse_args_from(args(&["wayland-ptt"])).unwrap();

    assert_eq!(parsed, config("grave", None));
}

#[test]
fn parses_all_supported_flags() {
    let parsed = parse_args_from(args(&[
        "wayland-ptt",
        "-v",
        "--trigger",
        "CTRL+Space",
        "--shortcut-id",
        "mumble-whisper",
        "--application",
        "wayland-ptt-mumble",
        "-s",
        "F13",
        "-t",
        "mumble",
    ]))
    .unwrap();

    assert_eq!(
        parsed,
        Config {
            xtest: true,
            verbose: true,
            trigger: "CTRL+Space".to_string(),
            shortcut_id: "mumble-whisper".to_string(),
            application: "wayland-ptt-mumble".to_string(),
            send_key: "F13".to_string(),
            mouse_button: None,
            target: "mumble".to_string(),
            vesktop_action: Some("pushToMute".to_string()),
        }
    );
}

#[test]
fn parses_mouse_target() {
    let parsed = parse_args_from(args(&["wayland-ptt", "-s", "MOUSE5"])).unwrap();

    assert_eq!(parsed, config("MOUSE5", Some(5)));
}

#[test]
fn send_event_selects_direct_backend() {
    let parsed = parse_args_from(args(&["wayland-ptt", "--send-event"])).unwrap();

    assert!(!parsed.xtest);
}

#[test]
fn rejects_invalid_mouse_target() {
    let err = parse_args_from(args(&["wayland-ptt", "-s", "MOUSEabc"])).unwrap_err();

    assert_eq!(err, "Invalid mouse_button value: MOUSEabc");
}

#[test]
fn rejects_mouse_zero_target() {
    let err = parse_args_from(args(&["wayland-ptt", "-s", "MOUSE0"])).unwrap_err();

    assert_eq!(err, "Invalid mouse_button value: MOUSE0");
}

#[test]
fn rejects_positional_arguments() {
    let err = parse_args_from(args(&["wayland-ptt", "/dev/input/event0"])).unwrap_err();

    assert_eq!(err, usage("wayland-ptt"));
}

#[test]
fn rejects_unknown_flag() {
    let err = parse_args_from(args(&["wayland-ptt", "-x"])).unwrap_err();

    assert_eq!(err, usage("wayland-ptt"));
}

#[test]
fn rejects_empty_trigger() {
    let err = parse_args_from(args(&["wayland-ptt", "--trigger", ""])).unwrap_err();

    assert_eq!(err, "Invalid --trigger value");
}

#[test]
fn rejects_empty_shortcut_id() {
    let err = parse_args_from(args(&["wayland-ptt", "--shortcut-id", ""])).unwrap_err();

    assert_eq!(err, "Invalid --shortcut-id value");
}

#[test]
fn no_vesktop_disables_action() {
    let parsed = parse_args_from(args(&["wayland-ptt", "--no-vesktop"])).unwrap();

    assert_eq!(parsed.vesktop_action, None);
}

#[test]
fn vesktop_action_is_configurable() {
    let parsed = parse_args_from(args(&["wayland-ptt", "--vesktop-action", "pushToTalk"])).unwrap();

    assert_eq!(parsed.vesktop_action, Some("pushToTalk".to_string()));
}
