//! The notification area icon and its menu.
//!
//! * The icon is drawn for the tray's pixel size (see `icons.rs`) in three
//!   shapes: normal, paused (a pause sign), recording (a record badge).
//! * The menu: a status line (Active, Paused or Hidden, the profile, the
//!   recording timecode), Pause or Resume, Keyboard visual, SolidWorks mode
//!   (Auto, On, Off), Native overlay, Start or Stop session log, Add chapter
//!   marker, Open settings, Quit.
//! * The tooltip mirrors the status line. A left click opens settings.
//!
//! Menu clicks arrive on the main thread and only queue an action. Updates
//! (`Tray::update`) wait for the main thread, so they are made from the
//! housekeeping thread and never while holding a lock.

use keystage_core::protocol::SolidworksMode;
use parking_lot::Mutex;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Runtime};

use super::actions::Action;
use super::icons;
use super::state::{mode_words, TrayGlyph};

pub const TRAY_ID: &str = "keystage";

const ID_STATUS: &str = "status";
const ID_PAUSE: &str = "pause";
const ID_KEYBOARD: &str = "keyboard";
const ID_SW_AUTO: &str = "sw-auto";
const ID_SW_ON: &str = "sw-on";
const ID_SW_OFF: &str = "sw-off";
const ID_NATIVE: &str = "native";
const ID_SESSION: &str = "session";
const ID_MARKER: &str = "marker";
const ID_SETTINGS: &str = "settings";
const ID_QUIT: &str = "quit";

/// Everything the tray shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayView {
    pub status_line: String,
    pub tooltip: String,
    pub glyph: TrayGlyph,
    pub size: u32,
    pub paused: bool,
    pub keyboard_visible: bool,
    pub mode: SolidworksMode,
    pub native_overlay: bool,
    pub session_active: bool,
    pub marker_enabled: bool,
}

/// The action for a menu item id.
pub fn menu_action(id: &str) -> Option<Action> {
    Some(match id {
        ID_PAUSE => Action::TogglePause,
        ID_KEYBOARD => Action::ToggleKeyboard,
        ID_SW_AUTO => Action::SetMode(SolidworksMode::Auto),
        ID_SW_ON => Action::SetMode(SolidworksMode::On),
        ID_SW_OFF => Action::SetMode(SolidworksMode::Off),
        ID_NATIVE => Action::ToggleNativeOverlay,
        ID_SESSION => Action::ToggleSession,
        ID_MARKER => Action::AddMarker,
        ID_SETTINGS => Action::OpenSettings,
        ID_QUIT => Action::Quit,
        _ => return None,
    })
}

pub fn pause_text(paused: bool) -> &'static str {
    if paused {
        "Resume"
    } else {
        "Pause"
    }
}

pub fn session_text(active: bool) -> &'static str {
    if active {
        "Stop session log"
    } else {
        "Start session log"
    }
}

fn image(glyph: TrayGlyph, size: u32) -> Image<'static> {
    Image::new_owned(icons::render(glyph, size), size, size)
}

/// The tray and the menu items it updates.
pub struct Tray<R: Runtime> {
    icon: TrayIcon<R>,
    status: MenuItem<R>,
    pause: MenuItem<R>,
    keyboard: CheckMenuItem<R>,
    modes: [(SolidworksMode, CheckMenuItem<R>); 3],
    native: CheckMenuItem<R>,
    session: MenuItem<R>,
    marker: MenuItem<R>,
    last: Mutex<Option<TrayView>>,
}

impl<R: Runtime> Tray<R> {
    /// Builds the tray. `on_action` runs on the main thread: it must only
    /// queue the action.
    pub fn build<F>(app: &AppHandle<R>, view: &TrayView, on_action: F) -> tauri::Result<Tray<R>>
    where
        F: Fn(&AppHandle<R>, Action) + Send + Sync + Clone + 'static,
    {
        let none: Option<&str> = None;
        let status = MenuItem::with_id(app, ID_STATUS, &view.status_line, false, none)?;
        let pause = MenuItem::with_id(app, ID_PAUSE, pause_text(view.paused), true, none)?;
        let keyboard = CheckMenuItem::with_id(
            app,
            ID_KEYBOARD,
            "Keyboard visual",
            true,
            view.keyboard_visible,
            none,
        )?;
        let mode_item = |id: &str, mode: SolidworksMode| {
            CheckMenuItem::with_id(app, id, mode_words(mode), true, view.mode == mode, none)
        };
        let auto = mode_item(ID_SW_AUTO, SolidworksMode::Auto)?;
        let on = mode_item(ID_SW_ON, SolidworksMode::On)?;
        let off = mode_item(ID_SW_OFF, SolidworksMode::Off)?;
        let sw =
            Submenu::with_id_and_items(app, "sw", "SolidWorks mode", true, &[&auto, &on, &off])?;
        let native = CheckMenuItem::with_id(
            app,
            ID_NATIVE,
            "Native overlay",
            true,
            view.native_overlay,
            none,
        )?;
        let session = MenuItem::with_id(
            app,
            ID_SESSION,
            session_text(view.session_active),
            true,
            none,
        )?;
        let marker = MenuItem::with_id(
            app,
            ID_MARKER,
            "Add chapter marker",
            view.marker_enabled,
            none,
        )?;
        let settings = MenuItem::with_id(app, ID_SETTINGS, "Open settings", true, none)?;
        let quit = MenuItem::with_id(app, ID_QUIT, "Quit IDEA Keystage", true, none)?;
        let sep1 = PredefinedMenuItem::separator(app)?;
        let sep2 = PredefinedMenuItem::separator(app)?;
        let sep3 = PredefinedMenuItem::separator(app)?;
        let menu = Menu::with_id_and_items(
            app,
            "tray",
            &[
                &status, &sep1, &pause, &keyboard, &sw, &native, &sep2, &session, &marker, &sep3,
                &settings, &quit,
            ],
        )?;
        let menu_cb = on_action.clone();
        let click_cb = on_action;
        let icon = TrayIconBuilder::with_id(TRAY_ID)
            .icon(image(view.glyph, view.size))
            .tooltip(&view.tooltip)
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(move |app, event| {
                if let Some(a) = menu_action(event.id().as_ref()) {
                    menu_cb(app, a);
                }
            })
            .on_tray_icon_event(move |tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    click_cb(tray.app_handle(), Action::OpenSettings);
                }
            })
            .build(app)?;
        Ok(Tray {
            icon,
            status,
            pause,
            keyboard,
            modes: [
                (SolidworksMode::Auto, auto),
                (SolidworksMode::On, on),
                (SolidworksMode::Off, off),
            ],
            native,
            session,
            marker,
            last: Mutex::new(Some(view.clone())),
        })
    }

    /// Forgets what is shown, so the next update sets every item (a click on
    /// a check item toggles it on its own).
    pub fn invalidate(&self) {
        *self.last.lock() = None;
    }

    /// Shows `view`, changing only what differs. Waits for the main thread:
    /// never call it from the main thread's event handlers or under a lock.
    pub fn update(&self, view: &TrayView) {
        let last = self.last.lock().clone();
        if last.as_ref() == Some(view) {
            return;
        }
        let all = last.is_none();
        let l = last.unwrap_or_else(|| view.clone());
        let mut ok = true;
        let mut check = |r: tauri::Result<()>| {
            if r.is_err() {
                ok = false;
            }
        };
        if all || l.glyph != view.glyph || l.size != view.size {
            check(self.icon.set_icon(Some(image(view.glyph, view.size))));
        }
        if all || l.tooltip != view.tooltip {
            check(self.icon.set_tooltip(Some(&view.tooltip)));
        }
        if all || l.status_line != view.status_line {
            check(self.status.set_text(&view.status_line));
        }
        if all || l.paused != view.paused {
            check(self.pause.set_text(pause_text(view.paused)));
        }
        if all || l.keyboard_visible != view.keyboard_visible {
            check(self.keyboard.set_checked(view.keyboard_visible));
        }
        if all || l.mode != view.mode {
            for (mode, item) in &self.modes {
                check(item.set_checked(*mode == view.mode));
            }
        }
        if all || l.native_overlay != view.native_overlay {
            check(self.native.set_checked(view.native_overlay));
        }
        if all || l.session_active != view.session_active {
            check(self.session.set_text(session_text(view.session_active)));
        }
        if all || l.marker_enabled != view.marker_enabled {
            check(self.marker.set_enabled(view.marker_enabled));
        }
        if ok {
            *self.last.lock() = Some(view.clone());
        } else {
            tracing::warn!("tray update failed");
            *self.last.lock() = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_map_to_actions() {
        assert_eq!(menu_action(ID_PAUSE), Some(Action::TogglePause));
        assert_eq!(
            menu_action(ID_SW_ON),
            Some(Action::SetMode(SolidworksMode::On))
        );
        assert_eq!(menu_action(ID_QUIT), Some(Action::Quit));
        assert_eq!(menu_action(ID_STATUS), None, "the status line does nothing");
        assert_eq!(menu_action("sw"), None);
        for id in [
            ID_KEYBOARD,
            ID_SW_AUTO,
            ID_SW_OFF,
            ID_NATIVE,
            ID_SESSION,
            ID_MARKER,
            ID_SETTINGS,
        ] {
            assert!(menu_action(id).is_some(), "{id}");
        }
    }

    #[test]
    fn labels_are_sentence_case() {
        assert_eq!(pause_text(false), "Pause");
        assert_eq!(pause_text(true), "Resume");
        assert_eq!(session_text(false), "Start session log");
        assert_eq!(session_text(true), "Stop session log");
    }
}
