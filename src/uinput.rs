use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const SYN_REPORT: u16 = 0;

const UINPUT_IOCTL_BASE: u64 = b'U' as u64;
const IOC_WRITE: u64 = 1;

const fn io(nr: u64) -> u64 {
    (UINPUT_IOCTL_BASE << 8) | nr
}

const fn iow(nr: u64, size: u64) -> u64 {
    (IOC_WRITE << 30) | (UINPUT_IOCTL_BASE << 8) | nr | (size << 16)
}

const UI_DEV_CREATE: u64 = io(1);
const UI_DEV_DESTROY: u64 = io(2);
const UI_DEV_SETUP: u64 = iow(3, core::mem::size_of::<UinputSetup>() as u64);
const UI_SET_EVBIT: u64 = iow(100, 4);
const UI_SET_KEYBIT: u64 = iow(101, 4);

const BUS_USB: u16 = 0x03;
const KEY_SPACE: u16 = 57;
const DEVICE_NAME: &[u8] = b"wayland-ptt";

#[repr(C)]
#[derive(Clone, Copy)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[repr(C)]
struct UinputSetup {
    id: InputId,
    name: [u8; 80],
    ff_effects_max: u32,
}

#[repr(C)]
struct InputEvent {
    sec: i64,
    usec: i64,
    event_type: u16,
    code: u16,
    value: i32,
}

#[derive(Debug)]
pub struct UinputError {
    message: String,
}

impl UinputError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for UinputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for UinputError {}

pub struct UinputKeyboard {
    file: File,
    code: u16,
}

impl UinputKeyboard {
    pub fn new(code: u16) -> Result<Self, UinputError> {
        let file = OpenOptions::new()
            .write(true)
            .open("/dev/uinput")
            .map_err(|source| UinputError::new(format!("Failed to open /dev/uinput: {source}")))?;
        let fd = file.as_raw_fd();

        unsafe {
            ioctl_value(fd, UI_SET_EVBIT, EV_KEY as i32)?;
            ioctl_value(fd, UI_SET_EVBIT, EV_SYN as i32)?;
            ioctl_value(fd, UI_SET_KEYBIT, i32::from(code))?;
            ioctl_value(fd, UI_SET_KEYBIT, i32::from(KEY_SPACE))?;

            let mut name = [0u8; 80];
            name[..DEVICE_NAME.len()].copy_from_slice(DEVICE_NAME);
            let mut setup = UinputSetup {
                id: InputId {
                    bustype: BUS_USB,
                    vendor: 0x1d6b,
                    product: 0x0104,
                    version: 1,
                },
                name,
                ff_effects_max: 0,
            };
            ioctl_ptr(fd, UI_DEV_SETUP, &mut setup)?;
            ioctl(fd, UI_DEV_CREATE)?;
        }

        Ok(Self { file, code })
    }

    pub fn code(&self) -> u16 {
        self.code
    }

    pub fn send(&mut self, pressed: bool) -> Result<(), UinputError> {
        let value = if pressed { 1 } else { 0 };
        self.write_event(EV_KEY, self.code, value)?;
        self.write_event(EV_SYN, SYN_REPORT, 0)
    }

    fn write_event(&mut self, event_type: u16, code: u16, value: i32) -> Result<(), UinputError> {
        let mut tv = std::mem::MaybeUninit::<libc::timeval>::uninit();
        unsafe {
            libc::gettimeofday(tv.as_mut_ptr(), std::ptr::null_mut());
            let tv = tv.assume_init();
            let event = InputEvent {
                sec: tv.tv_sec,
                usec: tv.tv_usec,
                event_type,
                code,
                value,
            };
            let bytes = std::slice::from_raw_parts(
                &event as *const InputEvent as *const u8,
                std::mem::size_of::<InputEvent>(),
            );
            self.file.write_all(bytes).map_err(|source| {
                UinputError::new(format!("Failed to write uinput event: {source}"))
            })?;
        }
        Ok(())
    }
}

impl Drop for UinputKeyboard {
    fn drop(&mut self) {
        unsafe {
            let _ = ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY);
        }
    }
}

pub fn evdev_code_for_x_keycode(keycode: u8) -> Option<u16> {
    keycode
        .checked_sub(8)
        .filter(|code| *code > 0)
        .map(u16::from)
}

unsafe fn ioctl(fd: i32, request: u64) -> Result<(), UinputError> {
    if unsafe { libc::ioctl(fd, request as libc::c_ulong) } < 0 {
        return Err(UinputError::new(format!(
            "uinput ioctl {request:#x} failed: {}",
            io::Error::last_os_error()
        )));
    }
    Ok(())
}

unsafe fn ioctl_value(fd: i32, request: u64, value: i32) -> Result<(), UinputError> {
    if unsafe { libc::ioctl(fd, request as libc::c_ulong, value) } < 0 {
        return Err(UinputError::new(format!(
            "uinput ioctl {request:#x} failed: {}",
            io::Error::last_os_error()
        )));
    }
    Ok(())
}

unsafe fn ioctl_ptr<T>(fd: i32, request: u64, arg: *mut T) -> Result<(), UinputError> {
    if unsafe { libc::ioctl(fd, request as libc::c_ulong, arg) } < 0 {
        return Err(UinputError::new(format!(
            "uinput ioctl {request:#x} failed: {}",
            io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evdev_code_is_x_keycode_minus_eight() {
        assert_eq!(evdev_code_for_x_keycode(49), Some(41));
        assert_eq!(evdev_code_for_x_keycode(38), Some(30));
        assert_eq!(evdev_code_for_x_keycode(8), None);
        assert_eq!(evdev_code_for_x_keycode(7), None);
    }

    #[test]
    fn uinput_setup_layout_matches_kernel() {
        assert_eq!(core::mem::size_of::<InputId>(), 8);
        assert_eq!(core::mem::size_of::<UinputSetup>(), 92);
        assert_eq!(UI_DEV_CREATE, 0x5501);
        assert_eq!(UI_DEV_DESTROY, 0x5502);
        assert_eq!(UI_SET_EVBIT, 0x40045564);
        assert_eq!(UI_SET_KEYBIT, 0x40045565);
    }
}
