#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffTone {
    Added,
    Modified,
    Deleted,
    Staged,
}

impl DiffTone {
    pub const fn sign(self) -> &'static str {
        match self {
            Self::Added => "+",
            Self::Modified | Self::Staged => "~",
            Self::Deleted => "-",
        }
    }

    pub const fn text_class(self) -> &'static str {
        match self {
            Self::Added => "text-ansi-2",
            Self::Modified => "text-ansi-3",
            Self::Deleted => "text-ansi-1",
            Self::Staged => "text-ansi-3/80",
        }
    }

    pub const fn row_class(self) -> &'static str {
        match self {
            Self::Added => "bg-ansi-2/[0.06] hover:bg-ansi-2/[0.10]",
            Self::Modified => "bg-ansi-3/[0.06] hover:bg-ansi-3/[0.10]",
            Self::Deleted => "bg-ansi-1/[0.06] hover:bg-ansi-1/[0.10]",
            Self::Staged => "bg-ansi-3/[0.035] hover:bg-ansi-3/[0.07]",
        }
    }

    pub const fn marker_class(self) -> &'static str {
        match self {
            Self::Added => "bg-ansi-2",
            Self::Modified | Self::Staged => "bg-ansi-3",
            Self::Deleted => "bg-ansi-1",
        }
    }
}
