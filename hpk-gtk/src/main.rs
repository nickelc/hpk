use gio;
use gtk;
use hpk;
use open;

use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;

use gio::prelude::*;
use gtk::prelude::*;

macro_rules! clone {
    (@param _) => ( _ );
    (@param $x:ident) => ( $x );
    ($($n:ident),+ => move || $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move || $body
        }
    );
    ($($n:ident),+ => move |$($p:tt),+| $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            move |$(clone!(@param $p),)+| $body
        }
    );
}

macro_rules! get_widget {
    ($builder:expr, $id:expr) => {
        $builder
            .get_object($id)
            .expect(concat!("Couldn't get ", $id))
    };
}

#[derive(Clone)]
enum Action {
    Extract {
        src_file: PathBuf,
        dest_dir: PathBuf,
    },
    Create {
        src_dir: PathBuf,
        dest_file: PathBuf,
    },
    ExtractionCompleted(PathBuf),
    CreationCompleted(PathBuf),
}

struct App {
    app: gtk::Application,
    window: gtk::ApplicationWindow,
    _extract_widget: ExtractWidget,
    _create_widget: CreateWidget,
    receiver: Receiver<Action>,
    sender: Sender<Action>,
}

#[derive(Clone)]
struct ExtractWidget {
    src_file: gtk::FileChooserButton,
    dest_dir: gtk::FileChooserButton,
    action: gio::SimpleAction,
}

impl ExtractWidget {
    fn get_data(&self) -> (Option<PathBuf>, Option<PathBuf>) {
        (self.src_file.get_filename(), self.dest_dir.get_filename())
    }
}

#[derive(Clone)]
struct CreateWidget {
    src_dir: gtk::FileChooserButton,
    dest_dir: gtk::FileChooserButton,
    filename: gtk::Entry,
    action: gio::SimpleAction,
}

impl CreateWidget {
    fn get_data(&self) -> (Option<PathBuf>, Option<PathBuf>, Option<String>) {
        (
            self.src_dir.get_filename(),
            self.dest_dir.get_filename(),
            self.filename.get_text(),
        )
    }
}

impl App {
    fn new() -> App {
        let application = gtk::Application::new("org.hpk", gio::ApplicationFlags::empty())
            .expect("Initialization failed...");

        let builder = gtk::Builder::new_from_string(include_str!("hpk.ui"));

        let extract_widget = ExtractWidget {
            src_file: get_widget!(builder, "src_file"),
            dest_dir: get_widget!(builder, "dest_dir"),
            action: gio::SimpleAction::new("extract", None),
        };
        let create_widget = CreateWidget {
            src_dir: get_widget!(builder, "src_dir"),
            dest_dir: get_widget!(builder, "dest_location"),
            filename: get_widget!(builder, "dest_filename"),
            action: gio::SimpleAction::new("create", None),
        };
        let window: gtk::ApplicationWindow = get_widget!(builder, "window");
        window.add_action(&extract_widget.action);
        window.add_action(&create_widget.action);

        let (sender, receiver) = channel();

        // setup: FileFilters {{{
        let all_filter = gtk::FileFilter::new();
        let hpk_filter = gtk::FileFilter::new();
        FileFilterExt::set_name(&all_filter, "All Files");
        FileFilterExt::set_name(&hpk_filter, "HPK Files");
        all_filter.add_pattern("*");
        hpk_filter.add_pattern("*.hpk");

        extract_widget.src_file.add_filter(&hpk_filter);
        extract_widget.src_file.add_filter(&all_filter);
        extract_widget.src_file.set_filter(&hpk_filter);
        // }}}

        // setup: ActionBar {{{
        let stack: gtk::Stack = get_widget!(builder, "stack");
        let action_bar: gtk::ActionBar = get_widget!(builder, "action_bar");
        let create_button: gtk::Button = get_widget!(builder, "create_button");
        let extract_button: gtk::Button = get_widget!(builder, "extract_button");

        action_bar.pack_end(&extract_button);
        stack.connect_property_visible_child_notify(
            clone!(action_bar, create_button, extract_button => move |stack| {
                match stack.get_visible_child_name().as_ref().map(|x| &**x) {
                    Some("create") => {
                        action_bar.remove(&extract_button);
                        action_bar.pack_end(&create_button);
                    },
                    Some("extract") => {
                        action_bar.remove(&create_button);
                        action_bar.pack_end(&extract_button);
                    },
                    _ => {},
                }
            }),
        );
        // }}}

        // setup: ExtractWidget {{{
        extract_widget
            .src_file
            .connect_file_set(clone!(extract_widget => move |_| {
                match extract_widget.get_data() {
                    (Some(_), Some(_)) => extract_widget.action.set_enabled(true),
                    _ => extract_widget.action.set_enabled(false),
                }
            }));
        extract_widget
            .dest_dir
            .connect_file_set(clone!(extract_widget => move |_| {
                match extract_widget.get_data() {
                    (Some(_), Some(_)) => extract_widget.action.set_enabled(true),
                    _ => extract_widget.action.set_enabled(false),
                }
            }));
        extract_widget
            .action
            .connect_activate(clone!(extract_widget, sender => move |_, _| {
                if let (Some(file), Some(dir)) = extract_widget.get_data() {
                    sender.send(Action::Extract { src_file: file, dest_dir: dir })
                        .expect("Couldn't send data to the channel");
                }
            }));
        extract_widget.action.set_enabled(false);
        // }}}

        // setup: CreateWidget {{{
        create_widget
            .src_dir
            .connect_file_set(clone!(create_widget => move |_| {
                match create_widget.get_data() {
                    (Some(_), Some(_), Some(ref n)) if !n.is_empty() => {
                        create_widget.action.set_enabled(true)
                    },
                    _ => create_widget.action.set_enabled(false),
                }
            }));
        create_widget
            .filename
            .connect_changed(clone!(create_widget => move |_| {
                match create_widget.get_data() {
                    (Some(_), Some(_), Some(ref n)) if !n.is_empty() => {
                        create_widget.action.set_enabled(true)
                    },
                    _ => create_widget.action.set_enabled(false),
                }
            }));
        create_widget
            .dest_dir
            .connect_file_set(clone!(create_widget => move |_| {
                match create_widget.get_data() {
                    (Some(_), Some(_), Some(ref n)) if !n.is_empty() => {
                        create_widget.action.set_enabled(true)
                    },
                    _ => create_widget.action.set_enabled(false),
                }
            }));
        create_widget
            .action
            .connect_activate(clone!(create_widget, sender => move |_, _| {
                if let (Some(src_dir), Some(location), Some(filename)) = create_widget.get_data() {
                    if let Some(name) = PathBuf::from(filename).file_name() {
                        let dest_file = location.join(name);
                        sender.send(Action::Create { src_dir, dest_file })
                            .expect("Couldn't send data to the channel");
                    }
                }
            }));
        create_widget.action.set_enabled(false);
        // }}}

        App {
            app: application,
            window,
            _extract_widget: extract_widget,
            _create_widget: create_widget,
            receiver,
            sender,
        }
    }

    fn run(self) {
        let app = self.app;
        let window = self.window;
        app.connect_activate(|_| {});
        app.connect_startup(clone!(app, window => move |_| {
            app.add_window(&window);
            window.connect_delete_event(clone!(app => move |_, _| {
                app.quit();
                Inhibit(false)
            }));

            window.show_all();
            window.activate();
        }));

        let quit_action = gio::SimpleAction::new("quit", None);
        quit_action.connect_activate(clone!(app => move |_, _| {
            app.quit();
        }));
        app.add_action(&quit_action);

        let dialog = new_progress_dialog(&window);

        let sender = self.sender;
        let receiver = self.receiver;
        gtk::idle_add(move || {
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(Action::Extract { src_file, dest_dir }) => {
                    dialog.present();
                    extract(sender.clone(), src_file, dest_dir);
                }
                Ok(Action::Create { src_dir, dest_file }) => {
                    dialog.present();
                    create(sender.clone(), src_dir, dest_file);
                }
                Ok(Action::ExtractionCompleted(dest_dir)) => {
                    dialog.hide();
                    open_extraction_completed_dialog(&window, &dest_dir);
                }
                Ok(Action::CreationCompleted(dest_file)) => {
                    dialog.hide();
                    open_created_successfully_dialog(&window, &dest_file);
                }
                Err(_) => {}
            }
            Continue(true)
        });

        ApplicationExtManual::run(&app, &[]);
    }
}

fn extract(sender: Sender<Action>, src_file: PathBuf, dest_dir: PathBuf) {
    thread::spawn(move || {
        let options = hpk::ExtractOptions::new();
        hpk::extract(&options, &src_file, &dest_dir).unwrap();

        sender
            .send(Action::ExtractionCompleted(dest_dir))
            .expect("Couldn't send data to the channel");
    });
}

fn create(sender: Sender<Action>, src_dir: PathBuf, dest_file: PathBuf) {
    thread::spawn(move || {
        let options = hpk::CreateOptions::new();
        hpk::create(&options, &src_dir, &dest_file).unwrap();

        sender
            .send(Action::CreationCompleted(dest_file))
            .expect("Couldn't send data to the channel");
    });
}

fn open_extraction_completed_dialog(window: &gtk::ApplicationWindow, dest_dir: &PathBuf) {
    let dialog = new_dialog(
        window,
        "Extraction completted successfully",
        &[("Close", 0), ("Show the Files", 1)],
        0,
    );
    if dialog.run() == 1 {
        show_folder(dest_dir);
    }
    dialog.close();
}

fn open_created_successfully_dialog(window: &gtk::ApplicationWindow, dest_file: &PathBuf) {
    let dialog = new_dialog(
        window,
        &format!("{:?} created successfully", dest_file.file_name().unwrap()),
        &[("Close", 0), ("Show Location", 1)],
        0,
    );
    if dialog.run() == 1 {
        show_folder(dest_file.parent().unwrap());
    }
    dialog.close();
}

fn new_dialog(
    window: &gtk::ApplicationWindow,
    title: &str,
    buttons: &[(&str, i32)],
    default_response: i32,
) -> gtk::MessageDialog {
    let dialog = gtk::MessageDialog::new(
        Some(window),
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Other,
        gtk::ButtonsType::None,
        title,
    );
    for btn in buttons {
        dialog.add_button(btn.0, btn.1);
    }
    dialog.set_default_response(default_response);

    dialog
}

fn new_progress_dialog(parent: &gtk::ApplicationWindow) -> gtk::MessageDialog {
    let dialog = gtk::MessageDialog::new(
        Some(parent),
        gtk::DialogFlags::all(),
        gtk::MessageType::Other,
        gtk::ButtonsType::None,
        "Please wait...",
    );
    dialog.connect_delete_event(|_, _| Inhibit(true));

    let spinner = gtk::Spinner::new();
    spinner.set_visible(true);
    spinner.start();
    let container = dialog
        .get_message_area()
        .unwrap()
        .downcast::<gtk::Container>()
        .unwrap();
    container.add(&spinner);

    dialog
}

fn show_folder<P: AsRef<Path>>(dir: P) {
    let file = gio::File::new_for_path(dir);
    open::that(&file.get_uri().unwrap()).expect("Failed to show folder");
}

fn main() {
    App::new().run();
}

// vim: fdm=marker
