/// The three top-level pages (UX-1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Amplifiers,
    MixEq,
    Search,
}

impl Page {
    pub const ALL: [Page; 3] = [Page::Amplifiers, Page::MixEq, Page::Search];

    pub fn label(self) -> &'static str {
        match self {
            Page::Amplifiers => "Amplifiers",
            Page::MixEq => "Mix & EQ",
            Page::Search => "Search",
        }
    }

    /// Whether the page draws meters, so that it is redrawn as they move (PERF-2).
    pub fn shows_meters(self) -> bool {
        matches!(self, Page::Amplifiers | Page::MixEq)
    }

    pub(crate) fn anchor(self) -> &'static str {
        match self {
            Page::Amplifiers => "#amplifiers",
            Page::MixEq => "#mix-eq",
            Page::Search => "#search",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PERF-2: the pages with meters follow them, the Search page has none
    #[test]
    fn the_pages_with_meters_are_redrawn_as_they_move() {
        assert!(Page::Amplifiers.shows_meters() && Page::MixEq.shows_meters());
        assert!(!Page::Search.shows_meters());
    }
}
