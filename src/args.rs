use std::env;

#[derive(Debug, PartialEq, Eq)]
pub struct Config {
    // Use XTEST by default so Mumble's XI2 raw-input shortcut engine sees the key.
    pub xtest: bool,
    pub verbose: bool,
    // Umbriel global-shortcut trigger, using its CTRL+ALT+SHIFT+LOGO+Key grammar.
    pub trigger: String,
    pub shortcut_id: String,
    pub application: String,
    // The key or MOUSE<n> button to send to X11 while the trigger is held.
    pub send_key: String,
    pub mouse_button: Option<u32>,
    // The application to send events to, matched against WM_CLASS and window titles.
    pub target: String,
    // Vesktop shortcut action triggered on the same chord (None disables it).
    pub vesktop_action: Option<String>,
}

pub fn usage(program: &str) -> String {
    format!(
        "Usage: {program} [-v] [--send-event] [--trigger TRIGGER] [--shortcut-id ID] [--application APP] [-s send_key] [-t target] [--vesktop-action ACTION] [--no-vesktop]"
    )
}

pub fn parse_args_from<I>(args: I) -> Result<Config, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let program = args.next().unwrap_or_else(|| "wayland-ptt".to_string());

    let mut verbose = false;
    let mut xtest = true;
    let mut trigger = "CTRL+grave".to_string();
    let mut shortcut_id = "push-to-talk".to_string();
    let mut application = "wayland-ptt".to_string();
    let mut send_key = "grave".to_string();
    let mut mouse_button = None;
    let mut target = "mumble".to_string();
    let mut vesktop_action = Some("pushToMute".to_string());

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-v" => verbose = true,
            "--send-event" => xtest = false,
            "--trigger" => {
                trigger = args.next().ok_or_else(|| usage(&program))?;
            }
            "--shortcut-id" => {
                shortcut_id = args.next().ok_or_else(|| usage(&program))?;
            }
            "--application" => {
                application = args.next().ok_or_else(|| usage(&program))?;
            }
            "-t" | "--target" => {
                target = args.next().ok_or_else(|| usage(&program))?;
                if target.is_empty() {
                    return Err(usage(&program));
                }
            }
            "-s" | "--send-key" => {
                let value = args.next().ok_or_else(|| usage(&program))?;
                mouse_button = value
                    .strip_prefix("MOUSE")
                    .map(|suffix| suffix.parse::<u32>())
                    .transpose()
                    .map_err(|_| format!("Invalid mouse_button value: {value}"))?;
                if mouse_button == Some(0) {
                    return Err("Invalid mouse_button value: MOUSE0".to_string());
                }
                send_key = value;
            }
            "--no-vesktop" => vesktop_action = None,
            "--vesktop-action" => {
                let value = args.next().ok_or_else(|| usage(&program))?;
                if value.is_empty() {
                    return Err("Invalid --vesktop-action value".to_string());
                }
                vesktop_action = Some(value);
            }
            _ if arg.starts_with('-') => return Err(usage(&program)),
            _ => return Err(usage(&program)),
        }
    }

    if trigger.is_empty() || trigger.len() > 256 {
        return Err("Invalid --trigger value".to_string());
    }
    if shortcut_id.is_empty() || shortcut_id.len() > 256 {
        return Err("Invalid --shortcut-id value".to_string());
    }
    if application.is_empty() || application.len() > 256 {
        return Err("Invalid --application value".to_string());
    }
    if send_key.is_empty() {
        return Err("Invalid -s/--send-key value".to_string());
    }

    Ok(Config {
        xtest,
        verbose,
        trigger,
        shortcut_id,
        application,
        send_key,
        mouse_button,
        target,
        vesktop_action,
    })
}

pub fn parse_args() -> Result<Config, String> {
    parse_args_from(env::args())
}
