use std::{
    ffi::{CStr, CString, c_char, c_double, c_int, c_void},
    ptr::NonNull,
};

const ERROR_BUFFER_SIZE: usize = 512;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeEventType {
    None = 0,
    FileLoaded = 1,
    PlaybackRestart = 2,
    PropertyChange = 3,
    EndFile = 4,
    Error = 5,
}

impl TryFrom<c_int> for NativeEventType {
    type Error = String;

    fn try_from(value: c_int) -> Result<Self, String> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::FileLoaded),
            2 => Ok(Self::PlaybackRestart),
            3 => Ok(Self::PropertyChange),
            4 => Ok(Self::EndFile),
            5 => Ok(Self::Error),
            _ => Err(format!("Unknown native event type: {value}")),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeProperty {
    None = 0,
    TimePos = 1,
    Duration = 2,
    Pause = 3,
    Volume = 4,
    Mute = 5,
}

impl TryFrom<c_int> for NativeProperty {
    type Error = String;

    fn try_from(value: c_int) -> Result<Self, String> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::TimePos),
            2 => Ok(Self::Duration),
            3 => Ok(Self::Pause),
            4 => Ok(Self::Volume),
            5 => Ok(Self::Mute),
            _ => Err(format!("Unknown native property: {value}")),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeEndReason {
    Unknown = 0,
    Eof = 1,
    Stop = 2,
    Quit = 3,
    Error = 4,
    Redirect = 5,
}

impl TryFrom<c_int> for NativeEndReason {
    type Error = String;

    fn try_from(value: c_int) -> Result<Self, String> {
        match value {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Eof),
            2 => Ok(Self::Stop),
            3 => Ok(Self::Quit),
            4 => Ok(Self::Error),
            5 => Ok(Self::Redirect),
            _ => Err(format!("Unknown native end reason: {value}")),
        }
    }
}

// Raw C ABI structure 
// Enums are represented as raw c_int to prevent undefined behavior from invalid C integers
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NativeEvent {
    pub event_type: c_int,
    pub load_id: u64,
    pub property: c_int,
    pub value: c_double,
    pub end_reason: c_int,
    pub error_code: c_int,
}

// Validated event payload for safe consumption by Rust code
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeEventData {
    pub event_type: NativeEventType,
    pub load_id: u64,
    pub property: NativeProperty,
    pub value: f64,
    pub end_reason: NativeEndReason,
    pub error_code: i32,
}

impl TryFrom<NativeEvent> for NativeEventData {
    type Error = String;

    fn try_from(raw: NativeEvent) -> Result<Self, String> {
        Ok(Self {
            event_type: NativeEventType::try_from(raw.event_type)?,
            load_id: raw.load_id,
            property: NativeProperty::try_from(raw.property)?,
            value: raw.value,
            end_reason: NativeEndReason::try_from(raw.end_reason)?,
            error_code: raw.error_code,
        })
    }
}

unsafe extern "C" {
    fn ansimuz_mpv_create(
        out_player: *mut *mut c_void,
        error_buffer: *mut c_char,
        error_buffer_size: usize,
    ) -> c_int;

    fn ansimuz_mpv_destroy(player: *mut c_void);

    fn ansimuz_mpv_loadfile(
        player: *mut c_void,
        path: *const c_char,
    ) -> c_int;

    fn ansimuz_mpv_get_last_load_id(player: *const c_void) -> u64;

    fn ansimuz_mpv_set_pause(
        player: *mut c_void,
        paused: c_int,
    ) -> c_int;

    fn ansimuz_mpv_stop(player: *mut c_void) -> c_int;

    fn ansimuz_mpv_seek(
        player: *mut c_void,
        seconds: c_double,
    ) -> c_int;

    fn ansimuz_mpv_get_time_pos(
        player: *mut c_void,
        out_seconds: *mut c_double,
    ) -> c_int;

    fn ansimuz_mpv_get_duration(
        player: *mut c_void,
        out_seconds: *mut c_double,
    ) -> c_int;

    fn ansimuz_mpv_set_volume(
        player: *mut c_void,
        volume: c_double,
    ) -> c_int;

    fn ansimuz_mpv_get_volume(
        player: *mut c_void,
        out_volume: *mut c_double,
    ) -> c_int;

    fn ansimuz_mpv_set_mute(
        player: *mut c_void,
        muted: c_int,
    ) -> c_int;

    fn ansimuz_mpv_get_mute(
        player: *mut c_void,
        out_muted: *mut c_int,
    ) -> c_int;

    fn ansimuz_mpv_error_string(status: c_int) -> *const c_char;

    fn ansimuz_mpv_poll_event(
        player: *mut c_void,
        out_event: *mut NativeEvent,
    ) -> c_int;
}

pub struct NativeMpv {
    handle: NonNull<c_void>,
}

impl NativeMpv {
    pub fn new() -> Result<Self, String> {
        let mut raw_player: *mut c_void = std::ptr::null_mut();
        let mut error_buffer = [0 as c_char; ERROR_BUFFER_SIZE];

        let status = unsafe {
            ansimuz_mpv_create(
                &mut raw_player,
                error_buffer.as_mut_ptr(),
                error_buffer.len(),
            )
        };

        if status < 0 {
            let message = unsafe {
                CStr::from_ptr(error_buffer.as_ptr())
                    .to_string_lossy()
                    .into_owned()
            };

            return Err(if message.is_empty() {
                format!("Native mpv initialization failed ({status})")
            } else {
                message
            });
        }

        let handle = NonNull::new(raw_player)
            .ok_or_else(|| "Native mpv returned a null player".to_string())?;

        Ok(Self { handle })
    }

    pub fn loadfile(&mut self, path: &str) -> Result<u64, String> {
        let path = CString::new(path)
            .map_err(|_| "Media path contains an interior NUL byte".to_string())?;

        let status = unsafe {
            ansimuz_mpv_loadfile(self.handle.as_ptr(), path.as_ptr())
        };

        self.check_status(status)?;

        Ok(unsafe {
            ansimuz_mpv_get_last_load_id(self.handle.as_ptr())
        })
    }

    pub fn set_pause(&mut self, paused: bool) -> Result<(), String> {
        let status = unsafe {
            ansimuz_mpv_set_pause(
                self.handle.as_ptr(),
                i32::from(paused),
            )
        };

        self.check_status(status)
    }

    pub fn stop(&mut self) -> Result<(), String> {
        let status = unsafe {
            ansimuz_mpv_stop(self.handle.as_ptr())
        };

        self.check_status(status)
    }

    pub fn seek(&mut self, seconds: f64) -> Result<(), String> {
        let status = unsafe {
            ansimuz_mpv_seek(self.handle.as_ptr(), seconds)
        };

        self.check_status(status)
    }

    pub fn time_pos(&self) -> Result<f64, String> {
        let mut seconds = 0.0;

        let status = unsafe {
            ansimuz_mpv_get_time_pos(
                self.handle.as_ptr(),
                &mut seconds,
            )
        };

        self.check_status(status)?;
        Ok(seconds)
    }

    pub fn duration(&self) -> Result<f64, String> {
        let mut seconds = 0.0;

        let status = unsafe {
            ansimuz_mpv_get_duration(
                self.handle.as_ptr(),
                &mut seconds,
            )
        };

        self.check_status(status)?;
        Ok(seconds)
    }

    pub fn set_volume(&mut self, volume: u8) -> Result<(), String> {
        let clamped_volume = volume.min(100) as f64;
        let status = unsafe {
            ansimuz_mpv_set_volume(self.handle.as_ptr(), clamped_volume)
        };

        self.check_status(status)
    }

    pub fn volume(&self) -> Result<f64, String> {
        let mut volume = 0.0;
        let status = unsafe {
            ansimuz_mpv_get_volume(self.handle.as_ptr(), &mut volume)
        };

        self.check_status(status)?;
        Ok(volume)
    }

    pub fn set_mute(&mut self, muted: bool) -> Result<(), String> {
        let status = unsafe {
            ansimuz_mpv_set_mute(
                self.handle.as_ptr(),
                i32::from(muted),
            )
        };

        self.check_status(status)
    }

    pub fn is_muted(&self) -> Result<bool, String> {
        let mut muted = 0;
        let status = unsafe {
            ansimuz_mpv_get_mute(self.handle.as_ptr(), &mut muted)
        };

        self.check_status(status)?;
        Ok(muted != 0)
    }

    pub fn poll_event(&mut self) -> Result<Option<NativeEventData>, String> {
        let mut event = NativeEvent {
            event_type: 0,
            load_id: 0,
            property: 0,
            value: 0.0,
            end_reason: 0,
            error_code: 0,
        };

        let status = unsafe {
            ansimuz_mpv_poll_event(
                self.handle.as_ptr(),
                &mut event,
            )
        };

        if status < 0 {
            return Err(self.error_message(status));
        }

        if status == 0 {
            return Ok(None);
        }

        let event_data = NativeEventData::try_from(event)?;
        Ok(Some(event_data))
    }

    fn check_status(&self, status: c_int) -> Result<(), String> {
        if status < 0 {
            Err(self.error_message(status))
        } else {
            Ok(())
        }
    }

    pub fn error_message(&self, status: c_int) -> String {
        unsafe {
            let ptr = ansimuz_mpv_error_string(status);

            if ptr.is_null() {
                return format!("Native mpv error: {status}");
            }

            CStr::from_ptr(ptr)
                .to_string_lossy()
                .into_owned()
        }
    }
}

impl Drop for NativeMpv {
    fn drop(&mut self) {
        unsafe {
            ansimuz_mpv_destroy(self.handle.as_ptr());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    #[test]
    fn test_native_event_abi_layout() {
        assert_eq!(size_of::<NativeEvent>(), 40);
        assert_eq!(align_of::<NativeEvent>(), 8);

        assert_eq!(offset_of!(NativeEvent, event_type), 0);
        assert_eq!(offset_of!(NativeEvent, load_id), 8);
        assert_eq!(offset_of!(NativeEvent, property), 16);
        assert_eq!(offset_of!(NativeEvent, value), 24);
        assert_eq!(offset_of!(NativeEvent, end_reason), 32);
        assert_eq!(offset_of!(NativeEvent, error_code), 36);
    }

    #[test]
    fn test_enum_conversions() {
        assert_eq!(NativeEventType::try_from(0).unwrap(), NativeEventType::None);
        assert_eq!(NativeEventType::try_from(1).unwrap(), NativeEventType::FileLoaded);
        assert_eq!(NativeEventType::try_from(2).unwrap(), NativeEventType::PlaybackRestart);
        assert_eq!(NativeEventType::try_from(3).unwrap(), NativeEventType::PropertyChange);
        assert_eq!(NativeEventType::try_from(4).unwrap(), NativeEventType::EndFile);
        assert_eq!(NativeEventType::try_from(5).unwrap(), NativeEventType::Error);
        assert!(NativeEventType::try_from(6).is_err());
        assert!(NativeEventType::try_from(-1).is_err());

        assert_eq!(NativeProperty::try_from(0).unwrap(), NativeProperty::None);
        assert_eq!(NativeProperty::try_from(1).unwrap(), NativeProperty::TimePos);
        assert_eq!(NativeProperty::try_from(2).unwrap(), NativeProperty::Duration);
        assert_eq!(NativeProperty::try_from(3).unwrap(), NativeProperty::Pause);
        assert_eq!(NativeProperty::try_from(4).unwrap(), NativeProperty::Volume);
        assert_eq!(NativeProperty::try_from(5).unwrap(), NativeProperty::Mute);
        assert!(NativeProperty::try_from(6).is_err());
        assert!(NativeProperty::try_from(-1).is_err());

        assert_eq!(NativeEndReason::try_from(0).unwrap(), NativeEndReason::Unknown);
        assert_eq!(NativeEndReason::try_from(1).unwrap(), NativeEndReason::Eof);
        assert_eq!(NativeEndReason::try_from(2).unwrap(), NativeEndReason::Stop);
        assert_eq!(NativeEndReason::try_from(3).unwrap(), NativeEndReason::Quit);
        assert_eq!(NativeEndReason::try_from(4).unwrap(), NativeEndReason::Error);
        assert_eq!(NativeEndReason::try_from(5).unwrap(), NativeEndReason::Redirect);
        assert!(NativeEndReason::try_from(6).is_err());
        assert!(NativeEndReason::try_from(-1).is_err());
    }

    #[test]
    fn test_event_data_conversion() {
        let raw = NativeEvent {
            event_type: 3,
            load_id: 42,
            property: 4,
            value: 85.0,
            end_reason: 0,
            error_code: 0,
        };

        let converted = NativeEventData::try_from(raw).expect("conversion should succeed");
        assert_eq!(converted.event_type, NativeEventType::PropertyChange);
        assert_eq!(converted.load_id, 42);
        assert_eq!(converted.property, NativeProperty::Volume);
        assert_eq!(converted.value, 85.0);
        assert_eq!(converted.end_reason, NativeEndReason::Unknown);
        assert_eq!(converted.error_code, 0);
    }
}
