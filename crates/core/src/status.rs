//! The status of one amplifier (BEH-8) as a pure state machine. The connection task of the amp is
//! the only one that feeds it events.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Online,
    Connecting,
    Reconnecting,
    Busy,
    Unreachable,
    CheckIp,
    Disconnected,
}

/// What happens to the connection of an amp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// The task begins an attempt chain: the amp is added, its IP edited, or CONNECT pressed.
    Start,
    /// The IP is not a valid IPv4 address.
    BadIp,
    /// The user released the amp.
    Stop,
    Connected,
    /// The connection of an ONLINE amp ended.
    Lost,
    /// An attempt failed because another controller holds the amp.
    OtherController,
    /// An attempt failed for any other reason.
    NoAnswer,
}

impl Status {
    /// The text of the status line.
    pub fn label(self) -> &'static str {
        match self {
            Status::Online => "ONLINE",
            Status::Connecting => "CONNECTING…",
            Status::Reconnecting => "RECONNECTING…",
            Status::Busy => "BUSY · OTHER CONTROLLER",
            Status::Unreachable => "UNREACHABLE · RETRYING",
            Status::CheckIp => "CHECK IP · INVALID ADDRESS",
            Status::Disconnected => "DISCONNECTED",
        }
    }

    pub(crate) fn next(self, event: Event) -> Status {
        use Status::*;
        match (self, event) {
            (_, Event::Stop) => Disconnected,
            (_, Event::BadIp) => CheckIp,
            (_, Event::Start) => Connecting,
            // results of an attempt the user has already abandoned
            (Disconnected | CheckIp, _) => self,
            (Online, Event::Lost) => Reconnecting,
            (Online, _) => Online,
            (_, Event::Connected) => Online,
            (Connecting | Reconnecting | Busy | Unreachable, Event::OtherController) => Busy,
            (Connecting | Reconnecting | Busy | Unreachable, Event::NoAnswer) => Unreachable,
            (_, Event::Lost) => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Event::*;
    use super::Status::*;
    use super::*;

    const ALL: [Status; 7] = [Online, Connecting, Reconnecting, Busy, Unreachable, CheckIp, Disconnected];

    // BEH-8
    #[test]
    fn labels_are_the_texts_of_the_table() {
        let labels = ALL.map(Status::label);
        assert_eq!(
            labels,
            [
                "ONLINE",
                "CONNECTING…",
                "RECONNECTING…",
                "BUSY · OTHER CONTROLLER",
                "UNREACHABLE · RETRYING",
                "CHECK IP · INVALID ADDRESS",
                "DISCONNECTED"
            ]
        );
    }

    // BEH-8, BEH-9: whatever it was, the user can release the amp
    #[test]
    fn stop_disconnects_from_every_status() {
        for s in ALL {
            assert_eq!(s.next(Stop), Disconnected, "{s:?}");
        }
    }

    // BEH-8, BEH-10: adding, editing the IP and CONNECT start from every status
    #[test]
    fn start_connects_from_every_status() {
        for s in ALL {
            assert_eq!(s.next(Start), Connecting, "{s:?}");
        }
    }

    // BEH-8: a bad IP is CHECK IP, from every status
    #[test]
    fn bad_ip_is_check_ip_from_every_status() {
        for s in ALL {
            assert_eq!(s.next(BadIp), CheckIp, "{s:?}");
        }
    }

    // BEH-8
    #[test]
    fn a_connection_that_succeeds_is_online() {
        for s in [Connecting, Reconnecting, Busy, Unreachable] {
            assert_eq!(s.next(Connected), Online, "{s:?}");
        }
        assert_eq!(Online.next(Connected), Online);
    }

    // BEH-8: a result that arrives after the user released the amp or the IP became invalid is stale
    #[test]
    fn results_after_stop_or_bad_ip_are_ignored() {
        for s in [Disconnected, CheckIp] {
            for e in [Connected, Lost, OtherController, NoAnswer] {
                assert_eq!(s.next(e), s, "{s:?} + {e:?}");
            }
        }
    }

    // BEH-8
    #[test]
    fn a_lost_connection_is_reconnecting() {
        assert_eq!(Online.next(Lost), Reconnecting);
        for s in [Connecting, Reconnecting, Busy, Unreachable, CheckIp, Disconnected] {
            assert_eq!(s.next(Lost), s, "{s:?}");
        }
    }

    // BEH-8: the status shows the outcome of the latest attempt
    #[test]
    fn a_failed_attempt_is_busy_or_unreachable() {
        for s in [Connecting, Reconnecting, Busy, Unreachable] {
            assert_eq!(s.next(OtherController), Busy, "{s:?}");
            assert_eq!(s.next(NoAnswer), Unreachable, "{s:?}");
        }
    }

    // BEH-8: a failure while ONLINE is not an attempt; the loss comes as `Lost`
    #[test]
    fn failures_do_not_touch_an_online_amp() {
        assert_eq!(Online.next(OtherController), Online);
        assert_eq!(Online.next(NoAnswer), Online);
    }

    // BEH-8: the full life of an amp
    #[test]
    fn a_typical_life() {
        let s = Disconnected.next(Start);
        assert_eq!(s, Connecting);
        let s = s.next(OtherController);
        assert_eq!(s, Busy);
        let s = s.next(Connected);
        assert_eq!(s, Online);
        let s = s.next(Lost);
        assert_eq!(s, Reconnecting);
        let s = s.next(NoAnswer);
        assert_eq!(s, Unreachable);
        let s = s.next(Connected);
        assert_eq!(s, Online);
        assert_eq!(s.next(Stop), Disconnected);
    }
}
