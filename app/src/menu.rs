use muda::{
    Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
    accelerator::{Accelerator, Code, Modifiers},
};
use winit::event_loop::EventLoopProxy;
#[derive(Clone, Copy, Debug)]
pub enum Action {
    MathReady,
    MlsReady,
    New,
    Open,
    Save,
    SaveAs,
    ExportPdf,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    InsertFormula,
    AskAiFormula,
    ToggleChart,
    Settings,
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
    let view = Submenu::new("View", true);
    let mut ids = vec![];
    for (parent, label, key, modifiers, action) in [
        (
            &app,
            "Settings…",
            Code::Comma,
            Modifiers::META,
            Action::Settings,
        ),
        (
            &app,
            "Quit Mathstral",
            Code::KeyQ,
            Modifiers::META,
            Action::Quit,
        ),
        (&file, "New", Code::KeyN, Modifiers::META, Action::New),
        (&file, "Open…", Code::KeyO, Modifiers::META, Action::Open),
        (
            &file,
            "Close Window",
            Code::KeyW,
            Modifiers::META,
            Action::Quit,
        ),
        (&file, "Save", Code::KeyS, Modifiers::META, Action::Save),
        (
            &file,
            "Save As…",
            Code::KeyS,
            Modifiers::META | Modifiers::SHIFT,
            Action::SaveAs,
        ),
        (
            &file,
            "Export as PDF…",
            Code::KeyE,
            Modifiers::META | Modifiers::ALT,
            Action::ExportPdf,
        ),
        (&edit, "Undo", Code::KeyZ, Modifiers::META, Action::Undo),
        (
            &edit,
            "Redo",
            Code::KeyZ,
            Modifiers::META | Modifiers::SHIFT,
            Action::Redo,
        ),
        (&edit, "Cut", Code::KeyX, Modifiers::META, Action::Cut),
        (&edit, "Copy", Code::KeyC, Modifiers::META, Action::Copy),
        (&edit, "Paste", Code::KeyV, Modifiers::META, Action::Paste),
        (
            &edit,
            "Select All",
            Code::KeyA,
            Modifiers::META,
            Action::SelectAll,
        ),
        (
            &edit,
            "Insert Formula",
            Code::Digit1,
            Modifiers::META,
            Action::InsertFormula,
        ),
        (
            &edit,
            "Ask AI for Formula",
            Code::Digit1,
            Modifiers::META | Modifiers::SHIFT,
            Action::AskAiFormula,
        ),
        (
            &view,
            "Structure",
            Code::KeyL,
            Modifiers::META,
            Action::ToggleChart,
        ),
    ] {
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
    menu.append_items(&[&app, &file, &edit, &view, &window])
        .unwrap();
    menu.init_for_nsapp();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some((_, action)) = ids.iter().find(|(id, _)| id == &event.id) {
            let _ = proxy.send_event(*action);
        }
    }));
    menu
}
