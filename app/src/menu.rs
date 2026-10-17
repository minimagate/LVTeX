use muda::{
    Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
    accelerator::{Accelerator, Code, Modifiers},
};
use winit::event_loop::EventLoopProxy;
#[derive(Clone, Copy, Debug)]
pub enum Action {
    MathReady,
    New,
    Open,
    Save,
    SaveAs,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Quit,
}
pub fn install(proxy: EventLoopProxy<Action>) -> Menu {
    let menu = Menu::new();
    let app = Submenu::new("Mathstral", true);
    app.append(&PredefinedMenuItem::hide(None)).unwrap();
    app.append(&PredefinedMenuItem::hide_others(None)).unwrap();
    app.append(&PredefinedMenuItem::show_all(None)).unwrap();
    app.append(&PredefinedMenuItem::separator()).unwrap();
    let file = Submenu::new("File", true);
    let edit = Submenu::new("Edit", true);
    let mut ids = vec![];
    for (parent, label, key, shift, action) in [
        (&app, "Quit Mathstral", Code::KeyQ, false, Action::Quit),
        (&file, "New", Code::KeyN, false, Action::New),
        (&file, "Open…", Code::KeyO, false, Action::Open),
        (&file, "Close Window", Code::KeyW, false, Action::Quit),
        (&file, "Save", Code::KeyS, false, Action::Save),
        (&file, "Save As…", Code::KeyS, true, Action::SaveAs),
        (&edit, "Undo", Code::KeyZ, false, Action::Undo),
        (&edit, "Redo", Code::KeyZ, true, Action::Redo),
        (&edit, "Cut", Code::KeyX, false, Action::Cut),
        (&edit, "Copy", Code::KeyC, false, Action::Copy),
        (&edit, "Paste", Code::KeyV, false, Action::Paste),
        (&edit, "Select All", Code::KeyA, false, Action::SelectAll),
    ] {
        let modifiers = if shift {
            Modifiers::META | Modifiers::SHIFT
        } else {
            Modifiers::META
        };
        let item = MenuItem::new(label, true, Some(Accelerator::new(modifiers, key)));
        ids.push((item.id().clone(), action));
        parent.append(&item).unwrap();
    }
    let window = Submenu::new("Window", true);
    window
        .append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(None),
            &PredefinedMenuItem::fullscreen(None),
        ])
        .unwrap();
    menu.append_items(&[&app, &file, &edit, &window]).unwrap();
    menu.init_for_nsapp();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some((_, action)) = ids.iter().find(|(id, _)| id == &event.id) {
            let _ = proxy.send_event(*action);
        }
    }));
    menu
}
