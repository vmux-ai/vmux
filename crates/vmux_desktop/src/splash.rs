use std::time::Duration;

const SPLASH_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplashAction {
    None,
    Fade,
    Force,
}

fn splash_decision(visible: bool, dismissed: bool, elapsed: Duration) -> SplashAction {
    if dismissed {
        return SplashAction::None;
    }
    if visible {
        return SplashAction::Fade;
    }
    if elapsed >= SPLASH_TIMEOUT {
        return SplashAction::Force;
    }
    SplashAction::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_within_timeout_does_nothing() {
        assert_eq!(
            splash_decision(false, false, Duration::from_secs(1)),
            SplashAction::None
        );
    }

    #[test]
    fn visible_triggers_fade() {
        assert_eq!(
            splash_decision(true, false, Duration::from_secs(1)),
            SplashAction::Fade
        );
    }

    #[test]
    fn hidden_past_timeout_forces_dismiss() {
        assert_eq!(
            splash_decision(false, false, Duration::from_secs(20)),
            SplashAction::Force
        );
    }

    #[test]
    fn dismissed_is_idempotent() {
        assert_eq!(
            splash_decision(true, true, Duration::from_secs(1)),
            SplashAction::None
        );
        assert_eq!(
            splash_decision(false, true, Duration::from_secs(99)),
            SplashAction::None
        );
    }
}
