use std::process;

use wayland_ptt::args::parse_args;
use wayland_ptt::uinput::{UinputKeyboard, evdev_code_for_x_keycode};
use wayland_ptt::umbriel;
use wayland_ptt::x11::{X11Target, configure_x11_target, configure_x11_with_backend};

fn main() {
    let config = match parse_args() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            process::exit(1);
        }
    };

    let mut x11_config = match configure_x11_with_backend(config.xtest) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{err}");
            process::exit(1);
        }
    };

    let x11_target = match configure_x11_target(&x11_config, &config) {
        Ok(target) => target,
        Err(err) => {
            eprintln!("{err}");
            process::exit(1);
        }
    };

    if config.xtest {
        if let X11Target::Key { keycode } = x11_target {
            if let Some(code) = evdev_code_for_x_keycode(keycode) {
                match UinputKeyboard::new(code) {
                    Ok(keyboard) => {
                        if config.verbose {
                            eprintln!("Using uinput virtual keyboard (evdev code {code})");
                        }
                        x11_config.set_uinput(keyboard);
                    }
                    Err(err) => {
                        if config.verbose {
                            eprintln!("uinput unavailable ({err}); using XTEST");
                        }
                    }
                }
            }
        }
    }

    if config.verbose {
        eprintln!(
            "Umbriel trigger {}, sending {} to {}",
            config.trigger, config.send_key, config.target
        );
    }

    if let Err(err) = umbriel::run(&config, x11_config, x11_target) {
        eprintln!("{err}");
        process::exit(1);
    }
}
