use std::fmt;

#[derive(Debug)]
pub enum KeyserError {
    Keychain(security_framework::base::Error),
    ItemNotFound,
}

impl fmt::Display for KeyserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyserError::Keychain(e) => write!(f, "Keychain error: {e}"),
            KeyserError::ItemNotFound => write!(f, "Item not found"),
        }
    }
}

impl std::error::Error for KeyserError {}

impl From<security_framework::base::Error> for KeyserError {
    fn from(e: security_framework::base::Error) -> Self {
        KeyserError::Keychain(e)
    }
}

pub type Result<T> = std::result::Result<T, KeyserError>;
