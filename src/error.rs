use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrthancError {
    #[error("uv error: {0}")]
    UvError(String),

    #[error("GitHub API error (status {status}): {message}")]
    GitHubError { status: u16, message: String },

    #[error("config error: {0}")]
    ConfigError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    SerdeError(String),

    #[error("{0}")]
    Other(String),
}

impl From<serde_json::Error> for OrthancError {
    fn from(err: serde_json::Error) -> Self {
        OrthancError::SerdeError(err.to_string())
    }
}

impl From<toml::de::Error> for OrthancError {
    fn from(err: toml::de::Error) -> Self {
        OrthancError::ConfigError(err.to_string())
    }
}

impl From<toml::ser::Error> for OrthancError {
    fn from(err: toml::ser::Error) -> Self {
        OrthancError::ConfigError(err.to_string())
    }
}

impl From<reqwest::Error> for OrthancError {
    fn from(err: reqwest::Error) -> Self {
        OrthancError::GitHubError {
            status: err.status().map_or(0, |s| s.as_u16()),
            message: err.to_string(),
        }
    }
}

impl From<git2::Error> for OrthancError {
    fn from(err: git2::Error) -> Self {
        OrthancError::Other(format!("git error: {}", err))
    }
}

impl From<octocrab::Error> for OrthancError {
    fn from(err: octocrab::Error) -> Self {
        match err {
            octocrab::Error::GitHub { source, .. } => OrthancError::GitHubError {
                status: source.status_code.as_u16(),
                message: source.message,
            },
            _ => OrthancError::GitHubError {
                status: 0,
                message: err.to_string(),
            },
        }
    }
}
