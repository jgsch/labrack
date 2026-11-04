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

    pub(crate) fn anchor(self) -> &'static str {
        match self {
            Page::Amplifiers => "#amplifiers",
            Page::MixEq => "#mix-eq",
            Page::Search => "#search",
        }
    }
}
