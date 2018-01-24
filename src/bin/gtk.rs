extern crate gio;
extern crate gtk;
extern crate hpk;

use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

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
    ($builder:expr, $id:expr) => (
        $builder.get_object($id).expect(concat!("Couldn't get ", $id))
    );
}

#[derive(Clone)]
struct App {
    app: gtk::Application,
    window: gtk::ApplicationWindow,
    extract_widget: ExtractWidget,
    create_widget: CreateWidget,
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
    fn new(app: &gtk::Application, builder: &gtk::Builder) -> App {
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
        app.add_window(&window);
        window.add_action(&extract_widget.action);
        window.add_action(&create_widget.action);

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
        extract_widget.action.set_enabled(false);

        extract_widget.src_file.connect_file_set(
            clone!(extract_widget => move |_| {
                match extract_widget.get_data() {
                    (Some(_), Some(_)) => extract_widget.action.set_enabled(true),
                    _ => extract_widget.action.set_enabled(false),
                }
            }),
        );
        extract_widget.dest_dir.connect_file_set(
            clone!(extract_widget => move |_| {
                match extract_widget.get_data() {
                    (Some(_), Some(_)) => extract_widget.action.set_enabled(true),
                    _ => extract_widget.action.set_enabled(false),
                }
            }),
        );
        // }}}

        // setup: CreateWidget {{{
        create_widget.action.set_enabled(false);

        create_widget.src_dir.connect_file_set(
            clone!(create_widget => move |_| {
                match create_widget.get_data() {
                    (Some(_), Some(_), Some(ref n)) if n.len() > 0 => {
                        create_widget.action.set_enabled(true)
                    },
                    _ => create_widget.action.set_enabled(false),
                }
            }),
        );
        create_widget.filename.connect_changed(
            clone!(create_widget => move |_| {
                match create_widget.get_data() {
                    (Some(_), Some(_), Some(ref n)) if n.len() > 0 => {
                        create_widget.action.set_enabled(true)
                    },
                    _ => create_widget.action.set_enabled(false),
                }
            }),
        );
        create_widget.dest_dir.connect_file_set(
            clone!(create_widget => move |_| {
                match create_widget.get_data() {
                    (Some(_), Some(_), Some(ref n)) if n.len() > 0 => {
                        create_widget.action.set_enabled(true)
                    },
                    _ => create_widget.action.set_enabled(false),
                }
            }),
        );
        // }}}

        App {
            app: app.clone(),
            window,
            extract_widget,
            create_widget,
        }
    }

    fn new_dialog(
        &self,
        title: &str,
        buttons: &[(&str, i32)],
        default_response: i32,
    ) -> gtk::MessageDialog {
        let dialog = gtk::MessageDialog::new(
            Some(&self.window),
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

    fn show_folder<P: AsRef<Path>>(&self, dir: P) {
        let file = gio::File::new_for_path(dir);
        gtk::show_uri(
            self.window.get_screen().as_ref(),
            &file.get_uri().unwrap(),
            0,
        ).expect("Failed to show folder");
    }
}

fn build_ui(app: &gtk::Application) {
    let builder = gtk::Builder::new_from_string(include_str!("hpk.ui"));

    let app = App::new(&app, &builder);

    app.extract_widget.action.connect_activate(
        clone!(app => move|_,_| {
            match app.extract_widget.get_data() {
                (Some(file), Some(dir)) => {
                    hpk::extract(file, dir.clone()).unwrap();
                    let dialog = app.new_dialog(
                        "Extraction completted successfully",
                        &[("Close", 0), ("Show the File", 1)],
                        0,
                    );
                    if dialog.run() == 1 {
                        app.show_folder(dir);
                    }
                    dialog.close();
                },
                _ => {},
            }
        }),
    );

    // Create
    app.create_widget.action.connect_activate(
        clone!(app => move |_,_| {
            match app.create_widget.get_data() {
                (Some(src_dir), Some(location), Some(filename)) => {
                    if let Some(name) = PathBuf::from(filename).file_name() {
                        let dest_file = location.join(name);
                        let mut file = File::create(dest_file).unwrap();
                        hpk::create(src_dir, &mut file).unwrap();
                        let dialog = app.new_dialog(
                            &format!("{:?} created successfully", name),
                            &[("Close", 0), ("Show Location", 1)],
                            0,
                        );
                        if dialog.run() == 1 {
                            app.show_folder(location);
                        }
                        dialog.close();
                    }
                },
                _ => {},
            }
        }),
    );

    app.window.connect_delete_event(|w, _| {
        w.destroy();
        Inhibit(false)
    });

    app.window.show_all();
    app.window.activate();
}

fn main() {
    let application = gtk::Application::new("org.hpk", gio::ApplicationFlags::empty())
        .expect("Initialization failed...");

    application.connect_activate(|_| {});
    application.connect_startup(move |app| build_ui(&app));

    let quit_action = gio::SimpleAction::new("quit", None);
    quit_action.connect_activate(clone!(application => move |_, _| {
        application.quit();
    }));
    application.add_action(&quit_action);

    ApplicationExtManual::run(&application, &[]);
}

// vim: fdm=marker
