//! What the tray menu and the global hotkeys can ask for. Both arrive on the
//! main thread, where nothing may block, so they only queue an [`Action`];
//! the action worker thread carries it out.

use keystage_core::protocol::SolidworksMode;

use super::hotkeys::HotkeyName;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    TogglePause,
    ToggleKeyboard,
    SetMode(SolidworksMode),
    /// The SolidWorks hotkey: Auto, On, Off, then Auto again.
    CycleMode,
    ToggleNativeOverlay,
    ToggleSession,
    AddMarker,
    OpenSettings,
    Quit,
}

impl Action {
    pub fn from_hotkey(name: HotkeyName) -> Action {
        match name {
            HotkeyName::Pause => Action::TogglePause,
            HotkeyName::ToggleKeyboard => Action::ToggleKeyboard,
            HotkeyName::ToggleSolidworks => Action::CycleMode,
            HotkeyName::ChapterMarker => Action::AddMarker,
            HotkeyName::SessionToggle => Action::ToggleSession,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hotkey_has_an_action() {
        let actions: Vec<Action> = HotkeyName::ALL
            .iter()
            .map(|n| Action::from_hotkey(*n))
            .collect();
        assert_eq!(
            actions,
            vec![
                Action::TogglePause,
                Action::ToggleKeyboard,
                Action::CycleMode,
                Action::AddMarker,
                Action::ToggleSession
            ]
        );
    }
}
