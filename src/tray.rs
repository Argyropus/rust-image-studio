//! Notification-area icon (Windows): the window's close button only hides the app, so a
//! generation in progress is not lost, and the process ends from the tray menu's Quit item.
//!
//! Other platforms keep the plain behaviour (the close button exits): the tray backends there
//! need GTK/libappindicator system libraries and a GTK event loop, which this build does not
//! pull in (`tray-icon` is a Windows-only dependency in Cargo.toml).

/// What the tray asked the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    /// Show and focus the window again (left click on the icon, or the menu's Open item).
    Show,
    /// End the process.
    Quit,
}

/// Platforms without a tray: no icon, so the close button keeps exiting the app.
#[cfg(not(windows))]
pub struct Tray;

#[cfg(not(windows))]
impl Tray {
    pub fn new(_ctx: &egui::Context) -> Option<Self> {
        None
    }

    pub fn take(&self) -> Option<TrayAction> {
        None
    }
}

#[cfg(windows)]
pub use win::Tray;

#[cfg(windows)]
mod win {
    use super::TrayAction;
    use std::sync::{Arc, Mutex};

    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    /// Stable identity for this build's icon: Windows then remembers where the user put it,
    /// instead of treating every launch as a new icon and hiding it under the overflow arrow.
    const TRAY_GUID: u128 = 0x5f3a_9c41_2b8e_4d76_9a10_c7e5_48b2_f03d;

    /// The notification-area icon. Kept alive for as long as the app runs: dropping it removes
    /// the icon from the notification area.
    pub struct Tray {
        _icon: TrayIcon,
        /// Requests pushed by the Win32 callbacks, picked up by the app's next turn.
        queue: Arc<Mutex<Vec<TrayAction>>>,
    }

    impl Tray {
        /// Creates the icon and its menu. `ctx` is only used to wake the app, so that a click is
        /// handled even while the window is hidden and no frame is being drawn.
        pub fn new(ctx: &egui::Context) -> Option<Self> {
            // Bilingual, because the labels are fixed once and the UI language can change later.
            let open = MenuItem::new("열기 (Open)", true, None);
            let quit = MenuItem::new("종료 (Quit)", true, None);
            let separator = PredefinedMenuItem::separator();
            let menu = Menu::new();
            menu.append_items(&[&open, &separator, &quit]).ok()?;
            let open_id = open.id().clone();
            let quit_id = quit.id().clone();

            let data = crate::window_icon();
            let icon = Icon::from_rgba(data.rgba, data.width, data.height).ok()?;
            let icon = TrayIconBuilder::new()
                .with_icon(icon)
                .with_menu(Box::new(menu))
                .with_tooltip(format!("{} — close hides, Quit here ends it", crate::APP_NAME))
                // A left click shows the window again; the menu opens on right click only.
                .with_menu_on_left_click(false)
                .with_guid(TRAY_GUID)
                .build()
                .ok()?;

            let queue: Arc<Mutex<Vec<TrayAction>>> = Arc::default();

            let click_queue = queue.clone();
            let click_ctx = ctx.clone();
            TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
                let clicked = match event {
                    TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } => true,
                    TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => true,
                    _ => false,
                };
                if clicked {
                    push(&click_queue, &click_ctx, TrayAction::Show);
                }
            }));

            let menu_queue = queue.clone();
            let menu_ctx = ctx.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let action = if event.id == open_id {
                    TrayAction::Show
                } else if event.id == quit_id {
                    TrayAction::Quit
                } else {
                    return;
                };
                push(&menu_queue, &menu_ctx, action);
            }));

            Some(Self { _icon: icon, queue })
        }

        /// The action the tray requested since the last call, if any.
        pub fn take(&self) -> Option<TrayAction> {
            let mut queue = self.queue.lock().ok()?;
            let mut action = None;
            for next in queue.drain(..) {
                match next {
                    // A Quit is never replaced by a click that arrived after it.
                    TrayAction::Quit => action = Some(TrayAction::Quit),
                    TrayAction::Show if action != Some(TrayAction::Quit) => action = Some(TrayAction::Show),
                    TrayAction::Show => {}
                }
            }
            action
        }
    }

    fn push(queue: &Arc<Mutex<Vec<TrayAction>>>, ctx: &egui::Context, action: TrayAction) {
        if let Ok(mut queue) = queue.lock() {
            queue.push(action);
        }
        // The window may be hidden: this wakes eframe so `App::logic` runs and applies the
        // request (it is the only app hook eframe calls while no frame is drawn).
        ctx.request_repaint();
    }
}
