use super::{
    events::PlayerEvent,
    native_mpv::{NativeEndReason, NativeEventData, NativeEventType, NativeProperty},
};

// Pure translator turning native mpv events into application PlayerEvents
// Correlates load IDs and track generations to drop stale events from previous tracks
pub struct Translator {
    current: Option<(u64, u64)>,
    duration: Option<f64>,
    last_pos_sent: f64,
    paused: bool,
}

impl Translator {
    pub fn new() -> Self {
        Self {
            current: None,
            duration: None,
            last_pos_sent: -1.0,
            paused: false,
        }
    }

    pub fn begin(&mut self, load_id: u64, generation: u64) {
        self.current = Some((load_id, generation));
        self.duration = None;
        self.last_pos_sent = -1.0;
        self.paused = false;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn translate(
        &mut self,
        ev: NativeEventData,
        describe: &dyn Fn(i32) -> String,
    ) -> Option<PlayerEvent> {
        let (load_id, generation) = self.current?;
        if ev.load_id != load_id {
            // Drop events belonging to a previous or different track
            return None;
        }

        match ev.event_type {
            NativeEventType::PropertyChange => match ev.property {
                NativeProperty::Duration => {
                    self.duration = (ev.value.is_finite() && ev.value > 0.0).then_some(ev.value);
                    None
                }
                NativeProperty::TimePos => {
                    if (ev.value - self.last_pos_sent).abs() < 0.25 {
                        return None;
                    }
                    self.last_pos_sent = ev.value;
                    Some(PlayerEvent::Position {
                        generation,
                        elapsed: ev.value,
                        duration: self.duration,
                    })
                }
                NativeProperty::Pause => {
                    self.paused = ev.value > 0.5;
                    None
                }
                _ => None,
            },
            NativeEventType::EndFile => match ev.end_reason {
                NativeEndReason::Eof => Some(PlayerEvent::Ended { generation }),
                NativeEndReason::Error => Some(PlayerEvent::Failed {
                    generation,
                    message: describe(ev.error_code),
                }),
                _ => None,
            },
            NativeEventType::Error => Some(PlayerEvent::Failed {
                generation,
                message: describe(ev.error_code),
            }),
            _ => None,
        }
    }
}

impl Default for Translator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaced_track_does_not_end_the_new_one() {
        let mut t = Translator::new();
        t.begin(2, 7);
        let old_stop = NativeEventData {
            event_type: NativeEventType::EndFile,
            load_id: 1,
            property: NativeProperty::None,
            value: 0.0,
            end_reason: NativeEndReason::Stop,
            error_code: 0,
        };
        assert_eq!(t.translate(old_stop, &|c| format!("err {c}")), None);
    }

    #[test]
    fn test_eof_produces_ended_event() {
        let mut t = Translator::new();
        t.begin(2, 7);
        let eof_event = NativeEventData {
            event_type: NativeEventType::EndFile,
            load_id: 2,
            property: NativeProperty::None,
            value: 0.0,
            end_reason: NativeEndReason::Eof,
            error_code: 0,
        };
        assert_eq!(
            t.translate(eof_event, &|c| format!("err {c}")),
            Some(PlayerEvent::Ended { generation: 7 })
        );
    }

    #[test]
    fn test_end_file_error_produces_failed_event() {
        let mut t = Translator::new();
        t.begin(2, 7);
        let err_event = NativeEventData {
            event_type: NativeEventType::EndFile,
            load_id: 2,
            property: NativeProperty::None,
            value: 0.0,
            end_reason: NativeEndReason::Error,
            error_code: -13,
        };
        assert_eq!(
            t.translate(err_event, &|c| format!("Playback error code {c}")),
            Some(PlayerEvent::Failed {
                generation: 7,
                message: "Playback error code -13".to_string(),
            })
        );
    }

    #[test]
    fn test_end_file_stop_current_load_produces_none() {
        let mut t = Translator::new();
        t.begin(2, 7);
        let stop_event = NativeEventData {
            event_type: NativeEventType::EndFile,
            load_id: 2,
            property: NativeProperty::None,
            value: 0.0,
            end_reason: NativeEndReason::Stop,
            error_code: 0,
        };
        assert_eq!(t.translate(stop_event, &|c| format!("err {c}")), None);
    }

    #[test]
    fn test_timepos_coalescing_and_filtering() {
        let mut t = Translator::new();
        t.begin(1, 42);

        let pos1 = NativeEventData {
            event_type: NativeEventType::PropertyChange,
            load_id: 1,
            property: NativeProperty::TimePos,
            value: 10.0,
            end_reason: NativeEndReason::Unknown,
            error_code: 0,
        };
        assert_eq!(
            t.translate(pos1, &|c| format!("err {c}")),
            Some(PlayerEvent::Position {
                generation: 42,
                elapsed: 10.0,
                duration: None,
            })
        );

        let pos2 = NativeEventData {
            event_type: NativeEventType::PropertyChange,
            load_id: 1,
            property: NativeProperty::TimePos,
            value: 10.1,
            end_reason: NativeEndReason::Unknown,
            error_code: 0,
        };
        assert_eq!(t.translate(pos2, &|c| format!("err {c}")), None);

        let pos3 = NativeEventData {
            event_type: NativeEventType::PropertyChange,
            load_id: 1,
            property: NativeProperty::TimePos,
            value: 10.3,
            end_reason: NativeEndReason::Unknown,
            error_code: 0,
        };
        assert_eq!(
            t.translate(pos3, &|c| format!("err {c}")),
            Some(PlayerEvent::Position {
                generation: 42,
                elapsed: 10.3,
                duration: None,
            })
        );
    }

    #[test]
    fn test_duration_property_stored_and_attached_to_position() {
        let mut t = Translator::new();
        t.begin(1, 10);

        let dur_event = NativeEventData {
            event_type: NativeEventType::PropertyChange,
            load_id: 1,
            property: NativeProperty::Duration,
            value: 215.5,
            end_reason: NativeEndReason::Unknown,
            error_code: 0,
        };
        assert_eq!(t.translate(dur_event, &|c| format!("err {c}")), None);

        let pos_event = NativeEventData {
            event_type: NativeEventType::PropertyChange,
            load_id: 1,
            property: NativeProperty::TimePos,
            value: 5.0,
            end_reason: NativeEndReason::Unknown,
            error_code: 0,
        };
        assert_eq!(
            t.translate(pos_event, &|c| format!("err {c}")),
            Some(PlayerEvent::Position {
                generation: 10,
                elapsed: 5.0,
                duration: Some(215.5),
            })
        );
    }

    #[test]
    fn test_pause_property_updates_internal_state() {
        let mut t = Translator::new();
        t.begin(1, 1);
        assert!(!t.is_paused());

        let pause_on = NativeEventData {
            event_type: NativeEventType::PropertyChange,
            load_id: 1,
            property: NativeProperty::Pause,
            value: 1.0,
            end_reason: NativeEndReason::Unknown,
            error_code: 0,
        };
        assert_eq!(t.translate(pause_on, &|c| format!("err {c}")), None);
        assert!(t.is_paused());

        let pause_off = NativeEventData {
            event_type: NativeEventType::PropertyChange,
            load_id: 1,
            property: NativeProperty::Pause,
            value: 0.0,
            end_reason: NativeEndReason::Unknown,
            error_code: 0,
        };
        assert_eq!(t.translate(pause_off, &|c| format!("err {c}")), None);
        assert!(!t.is_paused());
    }

    #[test]
    fn test_unrelated_event_types_ignored() {
        let mut t = Translator::new();
        t.begin(1, 1);

        let file_loaded = NativeEventData {
            event_type: NativeEventType::FileLoaded,
            load_id: 1,
            property: NativeProperty::None,
            value: 0.0,
            end_reason: NativeEndReason::Unknown,
            error_code: 0,
        };
        assert_eq!(t.translate(file_loaded, &|c| format!("err {c}")), None);
    }
}
