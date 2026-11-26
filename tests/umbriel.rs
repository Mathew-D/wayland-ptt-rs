use wayland_ptt::args::Config;
use wayland_ptt::umbriel::credential_path;

fn config() -> Config {
    Config {
        xtest: true,
        verbose: false,
        trigger: "CTRL+grave".to_string(),
        shortcut_id: "push-to-talk".to_string(),
        application: "wayland-ptt".to_string(),
        send_key: "grave".to_string(),
        mouse_button: None,
        target: "mumble".to_string(),
    }
}

#[test]
fn credential_path_is_scoped_to_application_and_shortcut() {
    let path = credential_path(&config());
    let file_name = path.file_name().unwrap().to_str().unwrap();

    assert!(path.parent().unwrap().ends_with("wayland-ptt"));
    assert!(file_name.starts_with("wayland-ptt-push-to-talk-"));
}
