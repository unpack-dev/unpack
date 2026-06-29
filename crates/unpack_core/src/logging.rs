#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfrastructureLogLevel {
    Error,
    Warn,
    Info,
    Log,
    Verbose,
}

impl InfrastructureLogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Log => "log",
            Self::Verbose => "verbose",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warn => 1,
            Self::Info => 2,
            Self::Log => 3,
            Self::Verbose => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfrastructureLogEvent {
    pub level: InfrastructureLogLevel,
    pub name: String,
    pub message: String,
}

impl InfrastructureLogEvent {
    pub(crate) fn new(
        level: InfrastructureLogLevel,
        name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level,
            name: name.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InfrastructureLoggingOptions {
    pub level: Option<InfrastructureLogLevel>,
}

impl InfrastructureLoggingOptions {
    pub fn disabled() -> Self {
        Self { level: None }
    }

    pub fn enabled(self, level: InfrastructureLogLevel) -> bool {
        self.level
            .map(|configured| level.rank() <= configured.rank())
            .unwrap_or(false)
    }
}
