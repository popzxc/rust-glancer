use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilityLevel {
    Private,
    Public,
    Crate,
    Super,
    Self_,
    Restricted(String),
    Unknown(String),
}

impl fmt::Display for VisibilityLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VisibilityLevel::Private => write!(f, "private"),
            VisibilityLevel::Public => write!(f, "pub"),
            VisibilityLevel::Crate => write!(f, "pub(crate)"),
            VisibilityLevel::Super => write!(f, "pub(super)"),
            VisibilityLevel::Self_ => write!(f, "pub(self)"),
            VisibilityLevel::Restricted(path) => write!(f, "pub(in {path})"),
            VisibilityLevel::Unknown(raw) => write!(f, "{raw}"),
        }
    }
}
