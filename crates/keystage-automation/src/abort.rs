//! The abort watch for automated input (docs/ARCHITECTURE.md section 14).
//!
//! While a script runs, any input a person makes takes over: the low-level
//! hooks mark input that `SendInput` made (`LLKHF_INJECTED`,
//! `LLMHF_INJECTED`), so every event without the mark is a person's, and
//! the platform layer counts them. A physical Escape is counted on its own
//! so the result can say so. The watch is sticky: once aborted it stays
//! aborted.

use serde::{Deserialize, Serialize};

/// What the app reports between two steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// Keyboard and mouse events without the injected mark, since start.
    pub human_events: u64,
    /// Physical Escape key presses, since start.
    pub human_escapes: u64,
    /// The hooks are installed (without them a person cannot be noticed).
    pub hooks_installed: bool,
    pub locked: bool,
    pub paused: bool,
    /// A password field has the focus (or a focus check is pending).
    pub password: bool,
    /// The foreground app is excluded or not identified.
    pub hidden_app: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbortReason {
    /// A person moved the mouse, clicked, scrolled or pressed a key.
    HumanInput,
    /// A person pressed Escape.
    Escape,
    HooksOff,
    Locked,
    Paused,
    /// Typing was about to go into a password field.
    PasswordField,
    /// Typing was about to go into an excluded or unidentified app.
    HiddenApp,
    /// The setting "Allow Claude to move the mouse and type" was turned off.
    NotAllowed,
}

impl AbortReason {
    pub fn message(self) -> &'static str {
        match self {
            AbortReason::HumanInput => "Stopped: someone used the mouse or keyboard, so the person took over.",
            AbortReason::Escape => "Stopped: Escape was pressed.",
            AbortReason::HooksOff => "Refused: input capture is not running, so a person taking over could not be noticed.",
            AbortReason::Locked => "Refused: the computer is locked.",
            AbortReason::Paused => "Refused: IDEA Keystage is paused, so a person taking over could not be noticed. Resume it first.",
            AbortReason::PasswordField => "Refused: a password field has the focus. IDEA Keystage never types into password fields.",
            AbortReason::HiddenApp => "Refused: the app in front is excluded or not identified yet, so nothing is typed there.",
            AbortReason::NotAllowed => "Refused: turn on \"Allow Claude to move the mouse and type\" in IDEA Keystage (General page or tray) first.",
        }
    }
}

/// One run's watch.
#[derive(Clone, Debug)]
pub struct Watch {
    base_events: u64,
    base_escapes: u64,
    aborted: Option<AbortReason>,
}

fn standing(s: &Snapshot) -> Option<AbortReason> {
    if !s.hooks_installed {
        Some(AbortReason::HooksOff)
    } else if s.locked {
        Some(AbortReason::Locked)
    } else if s.paused {
        Some(AbortReason::Paused)
    } else {
        None
    }
}

impl Watch {
    /// Starts a run, or says why it may not start.
    pub fn start(s: &Snapshot, allowed: bool) -> Result<Watch, AbortReason> {
        if !allowed {
            return Err(AbortReason::NotAllowed);
        }
        if let Some(r) = standing(s) {
            return Err(r);
        }
        Ok(Watch {
            base_events: s.human_events,
            base_escapes: s.human_escapes,
            aborted: None,
        })
    }

    pub fn aborted(&self) -> Option<AbortReason> {
        self.aborted
    }

    /// Before each step. `typing` is true before a key goes out.
    pub fn check(&mut self, s: &Snapshot, allowed: bool, typing: bool) -> Result<(), AbortReason> {
        if let Some(r) = self.aborted {
            return Err(r);
        }
        let r = if s.human_escapes > self.base_escapes {
            Some(AbortReason::Escape)
        } else if s.human_events > self.base_events {
            Some(AbortReason::HumanInput)
        } else if !allowed {
            Some(AbortReason::NotAllowed)
        } else if let Some(r) = standing(s) {
            Some(r)
        } else if typing && s.password {
            Some(AbortReason::PasswordField)
        } else if typing && s.hidden_app {
            Some(AbortReason::HiddenApp)
        } else {
            None
        };
        match r {
            Some(r) => {
                self.aborted = Some(r);
                Err(r)
            }
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok() -> Snapshot {
        Snapshot {
            hooks_installed: true,
            human_events: 40,
            human_escapes: 2,
            ..Snapshot::default()
        }
    }

    #[test]
    fn refuses_to_start_when_a_person_could_not_be_noticed() {
        assert!(Watch::start(&ok(), true).is_ok());
        assert_eq!(
            Watch::start(&ok(), false).err(),
            Some(AbortReason::NotAllowed)
        );
        let s = Snapshot {
            hooks_installed: false,
            ..ok()
        };
        assert_eq!(Watch::start(&s, true).err(), Some(AbortReason::HooksOff));
        let s = Snapshot {
            paused: true,
            ..ok()
        };
        assert_eq!(Watch::start(&s, true).err(), Some(AbortReason::Paused));
        let s = Snapshot {
            locked: true,
            ..ok()
        };
        assert_eq!(Watch::start(&s, true).err(), Some(AbortReason::Locked));
    }

    #[test]
    fn a_person_takes_over_and_it_stays_stopped() {
        let mut w = Watch::start(&ok(), true).expect("start");
        assert_eq!(w.check(&ok(), true, true), Ok(()));
        let moved = Snapshot {
            human_events: 41,
            ..ok()
        };
        assert_eq!(w.check(&moved, true, false), Err(AbortReason::HumanInput));
        assert_eq!(
            w.check(&ok(), true, false),
            Err(AbortReason::HumanInput),
            "sticky"
        );
        assert_eq!(w.aborted(), Some(AbortReason::HumanInput));
        let mut w = Watch::start(&ok(), true).expect("start");
        let esc = Snapshot {
            human_events: 41,
            human_escapes: 3,
            ..ok()
        };
        assert_eq!(
            w.check(&esc, true, false),
            Err(AbortReason::Escape),
            "Escape is named"
        );
    }

    #[test]
    fn typing_never_goes_into_a_password_field_or_a_hidden_app() {
        let mut w = Watch::start(&ok(), true).expect("start");
        let pw = Snapshot {
            password: true,
            ..ok()
        };
        assert_eq!(w.check(&pw, true, false), Ok(()), "pointing is fine");
        assert_eq!(w.check(&pw, true, true), Err(AbortReason::PasswordField));
        let mut w = Watch::start(&ok(), true).expect("start");
        let hidden = Snapshot {
            hidden_app: true,
            ..ok()
        };
        assert_eq!(w.check(&hidden, true, true), Err(AbortReason::HiddenApp));
        let mut w = Watch::start(&ok(), true).expect("start");
        assert_eq!(w.check(&ok(), false, false), Err(AbortReason::NotAllowed));
        let mut w = Watch::start(&ok(), true).expect("start");
        let locked = Snapshot {
            locked: true,
            ..ok()
        };
        assert_eq!(w.check(&locked, true, false), Err(AbortReason::Locked));
        for r in [
            AbortReason::HumanInput,
            AbortReason::PasswordField,
            AbortReason::NotAllowed,
        ] {
            assert!(r.message().ends_with('.'));
        }
    }
}
