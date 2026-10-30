use super::*;
use std::cell::RefCell;

actions!(
    macos,
    [Quit, Hide, HideOthers, ShowAll, Open, Close, Minimize]
);

pub(super) fn run(path: Option<PathBuf>) {
    let app = Application::new();
    let context = Rc::new(RefCell::new(None::<AsyncApp>));
    let pending = Rc::new(RefCell::new(Vec::<PathBuf>::new()));
    let open_context = context.clone();
    let open_pending = pending.clone();
    app.on_open_urls(move |urls| {
        for path in urls
            .into_iter()
            .filter_map(|value| url::Url::parse(&value).ok()?.to_file_path().ok())
        {
            if let Some(cx) = open_context.borrow().as_ref() {
                let _ = cx.update(|cx| open_path(path, cx));
            } else {
                open_pending.borrow_mut().push(path);
            }
        }
    });
    app.on_reopen(|cx| {
        if let Some(window) = cx.active_window().or_else(|| cx.windows().first().copied()) {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        } else {
            open_explorer(None, cx);
        }
        cx.activate(true);
    });
    app.run(move |cx| {
        *context.borrow_mut() = Some(cx.to_async());
        install_menus(cx);
        open_explorer(path, cx);
        for path in pending.borrow_mut().drain(..) {
            open_path(path, cx);
        }
        cx.activate(true);
    });
}

fn open_path(path: PathBuf, cx: &mut App) {
    // Reuse the initial empty window when Finder delivers a file after launch.
    for window in cx.windows() {
        if let Some(window) = window.downcast::<Explorer>() {
            let reused = window
                .update(cx, |view, window, cx| {
                    if view.program.is_none() && !view.loading {
                        view.load(path.clone(), cx);
                        window.activate_window();
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if reused {
                cx.activate(true);
                return;
            }
        }
    }
    open_explorer(Some(path), cx);
    cx.activate(true);
}

fn install_menus(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Open, cx| {
        // Menu actions can run inside the active window's update. Defer a
        // second update until that borrow has ended.
        cx.defer(|cx| {
            let window = cx
                .active_window()
                .and_then(|w| w.downcast::<Explorer>())
                .unwrap_or_else(|| open_explorer(None, cx));
            let _ = window.update(cx, |view, _, cx| view.open(cx));
        });
    });
    cx.on_action(|_: &Close, cx| {
        cx.defer(|cx| {
            if let Some(window) = cx.active_window() {
                let _ = window.update(cx, |_, window, _| window.remove_window());
            }
        });
    });
    cx.on_action(|_: &Minimize, cx| {
        cx.defer(|cx| {
            if let Some(window) = cx.active_window() {
                let _ = window.update(cx, |_, window, _| window.minimize_window());
            }
        });
    });
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("cmd-alt-h", HideOthers, None),
        KeyBinding::new("cmd-o", Open, None),
        KeyBinding::new("cmd-w", Close, None),
        KeyBinding::new("cmd-m", Minimize, None),
    ]);
    cx.set_menus(vec![
        Menu {
            name: "PTE Explorer".into(),
            items: vec![
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Hide PTE Explorer", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit PTE Explorer", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("Open…", Open),
                MenuItem::action("Close Window", Close),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![MenuItem::action("Minimize", Minimize)],
        },
    ]);
}

#[cfg(all(test, feature = "ui-tests"))]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[gpui::test]
    fn close_menu_action_can_update_the_dispatching_window(cx: &mut TestAppContext) {
        cx.update(install_menus);
        let (_, cx) = cx.add_window_view(|_, _| Explorer::empty());
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 1);
        // Dispatch through the window, as AppKit menu commands do. The global
        // handler must wait until this window update ends before closing it.
        cx.dispatch_action(Close);
        cx.run_until_parked();
        assert!(cx.windows().is_empty());
    }
}
