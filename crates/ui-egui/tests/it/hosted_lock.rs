//! A hosted page (`files_locked`): the person can't create, open, close or recover a document in
//! its place, by any route; the host's own open (through the inbox) and its commands (`execute`)
//! still work, and dropped pictures still insert.

use egui_kittest::kittest::{By, NodeT, Queryable};
use serde_json::json;
use wordcraft_doc::{Document, StoryRef};
use wordcraft_engine::Session;
use wordcraft_ui_egui::{Inbox, Services, WordApp};

/// The commands that would put another document in the hosted one's place.
const SWAPS: [&str; 6] = ["file.new", "file.newFromTemplate", "file.open", "file.close", "file.recover", "ui.openFileDialog"];

fn app(locked: bool) -> WordApp {
    let mut app = WordApp::new(Session::new(Document::new()), Services::default());
    // The labels below are English; an integration test otherwise follows the machine's language.
    app.ui.language = "en".into();
    app.files_locked = locked;
    app
}

fn harness(app: WordApp) -> egui_kittest::Harness<'static, WordApp> {
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut WordApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    for _ in 0..6 {
        h.step();
    }
    h
}

fn press(h: &mut egui_kittest::Harness<'static, WordApp>, key: egui::Key) {
    let modifiers = egui::Modifiers::COMMAND;
    h.event(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
    h.event(egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
    for _ in 0..3 {
        h.step();
    }
}

fn png() -> Vec<u8> {
    let img = image::RgbaImage::from_fn(20, 10, |_, _| image::Rgba([200, 30, 30, 255]));
    let mut b = Vec::new();
    image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut b), image::ImageFormat::Png).unwrap();
    b
}

fn object_count(a: &mut WordApp) -> usize {
    a.session.run("arrange.selectionPane", &json!({})).unwrap().as_array().map(|x| x.len()).unwrap_or(0)
}

fn body(a: &WordApp) -> String {
    a.session.doc.plain_text(StoryRef::Body)
}

#[test]
fn locked_run_refuses_document_swaps() {
    for id in SWAPS {
        let mut a = app(true);
        let document = a.session.document_id();
        let refused = a.run(id, json!({}));
        assert!(refused.is_err(), "{id} is refused while locked: {refused:?}");
        assert_eq!(a.session.documents().len(), 1, "{id}: one document");
        assert_eq!(a.session.document_id(), document, "{id}: the same document");
        assert!(a.dialog.is_none(), "{id}: no dialog");
        assert!(!a.ui.backstage, "{id}: no File page");
        // The host's own path (control channel, the page's `execute`) is not refused.
        let mut a = app(true);
        let r = a.execute(id, json!({}));
        assert!(r != refused, "{id} through execute is not refused: {r:?}");
    }
    // The host's own New really makes a document.
    let mut a = app(true);
    a.execute("file.new", json!({})).unwrap();
    assert_eq!(a.session.documents().len(), 2, "file.new through execute makes a document");
}

#[test]
fn locked_shortcuts_do_not_new_open_or_close() {
    let mut h = harness(app(true));
    let document = h.state().session.document_id();
    for key in [egui::Key::N, egui::Key::O, egui::Key::W] {
        press(&mut h, key);
        let a = h.state();
        assert_eq!(a.session.documents().len(), 1, "{key:?}: one document");
        assert_eq!(a.session.document_id(), document, "{key:?}: the same document");
        assert!(a.dialog.is_none(), "{key:?}: no dialog");
        assert!(!a.quit_requested, "{key:?}: the window stays");
    }
    // The same key unlocked makes a new document, so the shortcut did reach the app above.
    let mut h = harness(app(false));
    press(&mut h, egui::Key::N);
    assert_eq!(h.state().session.documents().len(), 2, "Ctrl+N unlocked makes a new document");
}

#[test]
fn locked_backstage_hides_new_open_and_close() {
    let mut a = app(true);
    a.run("ui.tab", json!({"tab": "File"})).unwrap();
    // Print, Export and Save As live on File: it opens, on a page that swaps nothing.
    assert!(a.ui.backstage);
    let mut h = harness(a);
    assert_eq!(h.state().ui.backstage_page, "info");
    let buttons = |h: &egui_kittest::Harness<'static, WordApp>| -> Vec<String> {
        h.query_all(By::new().role(egui::accesskit::Role::Button)).filter_map(|n| n.accesskit_node().label().map(|l| l.trim().to_string())).collect()
    };
    let shown = buttons(&h);
    for page in ["Save As", "Print", "Export"] {
        assert!(shown.iter().any(|l| l == page), "{page} is on the File page: {shown:?}");
    }
    for page in ["Home", "New", "Open", "Close", "Recover"] {
        assert!(!shown.iter().any(|l| l == page), "{page} is not on the File page: {shown:?}");
    }
    // A page asked for by name (a keytip, a script) doesn't show either.
    for page in ["home", "new", "open", "recover"] {
        h.state_mut().run("ui.backstage", json!({"value": true, "page": page})).unwrap();
        h.step();
        h.step();
        assert_eq!(h.state().ui.backstage_page, "info", "{page} is not shown");
    }
}

#[test]
fn locked_recent_does_not_open() {
    let dir = std::env::temp_dir().join(format!("wordcraft-hosted-recent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("recent.docx");
    std::fs::write(&path, wordcraft_engine::io::save_bytes("recent.docx", &Document::from_text("from recent")).unwrap()).unwrap();
    let path = path.to_string_lossy().to_string();
    let mut a = app(true);
    a.ui.recent = vec![path.clone()];
    let document = a.session.document_id();
    assert!(wordcraft_ui_egui::backstage::open_recent(&mut a, &path).is_err());
    assert_eq!(a.session.documents().len(), 1);
    assert_eq!(a.session.document_id(), document);
    assert!(!body(&a).contains("from recent"));
    // Unlocked, the same entry opens.
    let mut a = app(false);
    wordcraft_ui_egui::backstage::open_recent(&mut a, &path).unwrap();
    assert!(body(&a).contains("from recent"));
    let _ = std::fs::remove_dir_all(&dir);
}

fn docx(text: &str) -> Vec<u8> {
    wordcraft_engine::io::save_bytes("stored.docx", &Document::from_text(text)).unwrap()
}

/// A hosted app: the person's files come through `inbox`, the host's through `host_open`.
fn hosted() -> (WordApp, Inbox, Inbox) {
    let (inbox, host_open) = (Inbox::default(), Inbox::default());
    let mut a = app(true);
    a.services.inbox = Some(inbox.clone());
    a.services.host_open = Some(host_open.clone());
    (a, inbox, host_open)
}

#[test]
fn locked_inbox_opens_host_document_in_place_of_blank() {
    let (a, _, host_open) = hosted();
    let mut h = harness(a);
    host_open.lock().unwrap().push(("stored.docx".into(), docx("stored text")));
    for _ in 0..3 {
        h.step();
    }
    let a = h.state();
    assert_eq!(a.session.documents().len(), 1, "the host's document replaced the blank one");
    assert!(body(a).contains("stored text"), "the host's document is shown: {:?}", body(a));
    assert!(!a.ui.backstage);
}

#[test]
fn locked_picker_document_is_refused() {
    let (a, inbox, _) = hosted();
    let mut h = harness(a);
    h.state_mut().session.run("text.insert", &json!({"text": "mine"})).unwrap();
    let document = h.state().session.document_id();
    // A picker (Insert Picture's All files, say) or a drop delivers a document.
    inbox.lock().unwrap().push(("picked.docx".into(), docx("picked text")));
    for _ in 0..3 {
        h.step();
    }
    let a = h.state();
    assert_eq!(a.session.documents().len(), 1, "one document");
    assert_eq!(a.session.document_id(), document, "the same document");
    assert_eq!(body(a), "mine", "unchanged");
    assert!(a.dialog.is_none());
    assert!(inbox.lock().unwrap().is_empty(), "the file was taken, not kept for later");
}

#[test]
fn locked_host_open_replaces_an_edited_blank() {
    let (a, _, host_open) = hosted();
    let mut h = harness(a);
    // The person types before the host's document arrives.
    h.state_mut().run("text.insert", json!({"text": "typed early"})).unwrap();
    assert!(h.state().session.dirty);
    host_open.lock().unwrap().push(("stored.docx".into(), docx("stored text")));
    for _ in 0..3 {
        h.step();
    }
    let a = h.state();
    assert_eq!(a.session.documents().len(), 1, "the host's document replaced the edited one");
    assert!(body(a).contains("stored text"), "the host's document is shown: {:?}", body(a));
    assert!(!body(a).contains("typed early"));
    assert!(a.dialog.is_none(), "no Save Changes prompt");
}

#[test]
fn locked_inbox_still_inserts_pictures() {
    let inbox = Inbox::default();
    let mut a = app(true);
    a.services.inbox = Some(inbox.clone());
    let mut h = harness(a);
    inbox.lock().unwrap().push(("drop.png".into(), png()));
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().session.documents().len(), 1);
    assert_eq!(object_count(h.state_mut()), 1, "the picture went into the document");
}

#[test]
fn unlocked_new_still_works() {
    let mut a = app(false);
    a.run("file.new", json!({})).unwrap();
    assert_eq!(a.session.documents().len(), 2, "New makes a document");
    let mut h = harness(app(false));
    h.state_mut().run("ui.tab", json!({"tab": "File"})).unwrap();
    h.step();
    h.step();
    assert_eq!(h.state().ui.backstage_page, "home", "File opens on Home");
}

#[test]
fn locked_mail_merge_does_not_open_a_document() {
    let merges = [
        ("mailings.finish", json!({"records": "all"})),
        ("mailings.finish", json!({})),
        ("mailings.printDocuments", json!({"records": "all"})),
        ("mailings.labels", json!({"text": "Ada"})),
        ("mailings.labels", json!({})),
        ("mailings.envelopes", json!({"delivery": "Ada\nOne Street"})),
        ("mailings.envelopes", json!({"delivery": "Ada\nOne Street", "add": false})),
    ];
    let with_recipients = |locked: bool| {
        let mut a = app(locked);
        a.session.run("text.insert", &json!({"text": "Dear friend"})).unwrap();
        a.execute("mailings.recipients", json!({"rows": [{"Name": "Ada"}, {"Name": "Grace"}]})).unwrap();
        a
    };
    for (id, params) in &merges {
        let mut a = with_recipients(true);
        let document = a.session.document_id();
        let r = a.run(id, params.clone());
        assert!(r.is_err(), "{id} {params} is refused while locked: {r:?}");
        assert_eq!(a.session.documents().len(), 1, "{id} {params}: one document");
        assert_eq!(a.session.document_id(), document, "{id} {params}: the same document");
        assert!(a.dialog.is_none(), "{id} {params}: no dialog");
        // Unlocked, the same merge makes a new document (a bare call opens its dialog instead).
        let mut a = with_recipients(false);
        a.run(id, params.clone()).unwrap();
        if a.dialog.is_none() {
            assert_eq!(a.session.documents().len(), 2, "{id} {params} unlocked makes a document");
        }
    }
    // An envelope added to the hosted document stays allowed, and Envelopes still opens its dialog.
    let mut a = with_recipients(true);
    a.run("mailings.envelopes", json!({"delivery": "Ada\nOne Street", "add": true})).unwrap();
    assert_eq!(a.session.documents().len(), 1);
    assert!(body(&a).contains("One Street"), "the envelope is in the hosted document");
    let mut a = with_recipients(true);
    a.run("mailings.envelopes", json!({})).unwrap();
    assert!(a.dialog.is_some(), "the Envelopes dialog opens");
    assert_eq!(a.session.documents().len(), 1);
    // The host's own path (`execute`) is not refused.
    let mut a = with_recipients(true);
    a.execute("mailings.finish", json!({"records": "all"})).unwrap();
    assert_eq!(a.session.documents().len(), 2);
}
