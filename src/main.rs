#![recursion_limit = "512"]

use std::cell::RefCell;
use std::rc::Rc;

use glib::{
    source::{Continue, SourceId},
    MainContext,
};

use vgtk::grid::GridProps;
use vgtk::lib::gio::{ActionExt, ApplicationFlags, File, FileExt, SimpleAction};
use vgtk::lib::glib::Error;
use vgtk::lib::gtk::{
    prelude::*, Align, Application, ApplicationWindow, Button, ButtonsType, DialogFlags, Entry,
    EntryExt, FileChooserAction, FileChooserNative, Grid, HeaderBar, Label, ListBox, ListBoxRow,
    MessageType, ResponseType, ScrolledWindow, SelectionMode, Window,
};
use vgtk::{ext::*, gtk, on_signal, run, Component, UpdateAction, VNode};

mod cargo;
mod rust;
mod watcher;

use crate::cargo::CompileResult;
use crate::watcher::Watcher;

#[derive(Clone, Debug)]
enum AppState {
    Idle,
    Watching,
}

impl Default for AppState {
    fn default() -> Self {
        AppState::Idle
    }
}

impl AppState {
    fn map<T, F1, F2>(&self, on_idle: F1, on_watching: F2) -> T
    where
        F1: Fn() -> T,
        F2: Fn() -> T,
    {
        match self {
            AppState::Idle => on_idle(),
            AppState::Watching => on_watching(),
        }
    }
}

#[derive(Clone, Debug)]
enum Message {
    NoOp,
    FolderSelected(String),
    SelectFolder,
    FileError(Error),
    PathChanged(String),
    CommandChanged(String),
    ToggleWatch,
    ClearOutput,
    Exit,
}

struct Model {
    project_root: String,
    command: String,
    results: Option<Rc<RefCell<CompileResult>>>,
    state: AppState,
    watcher: Option<Watcher>,
    receiver_id: Option<SourceId>,
}

impl Default for Model {
    fn default() -> Self {
        Model {
            project_root: "/home/avranju/code/glib-channel".to_string(),
            command: "cargo check".to_string(),
            results: None,
            state: AppState::default(),
            watcher: None,
            receiver_id: None,
        }
    }
}

impl Model {
    fn render_results<'a>(&'a self) -> impl Iterator<Item = VNode<Model>> + 'a {
        self.results
            .iter()
            .flat_map(|v| {
                let result = v.borrow().clone();
                let output = if result.success {
                    "Compile succeeded.".to_string()
                } else {
                    "Compile failed.".to_string()
                };

                result
                    .errors
                    .into_iter()
                    .map(|d| d.to_string())
                    .chain(result.warnings.into_iter().map(|d| d.to_string()))
                    .chain(vec![output])
            })
            .map(|result| {
                let label = format!("<span font_family=\"monospace\">{}</span>", result);
                gtk! {
                    <ListBoxRow>
                        <Label label=label use_markup=true />
                    </ListBoxRow>
                }
            })
    }
}

impl Component for Model {
    type Message = Message;
    type Properties = ();

    fn update(&mut self, msg: Self::Message) -> UpdateAction<Self> {
        match msg {
            Message::NoOp => UpdateAction::None,

            Message::FileError(error) => UpdateAction::defer(async move {
                vgtk::message_dialog(
                    vgtk::current_window().as_ref(),
                    DialogFlags::empty(),
                    MessageType::Error,
                    ButtonsType::Ok,
                    true,
                    format!("<b>AN ERROR HAS OCCURRED!</b>\n\n{}", error),
                )
                .await;
                Message::NoOp
            }),

            Message::SelectFolder => UpdateAction::defer(async {
                match select_folder().await {
                    Ok(Some(file)) => Message::FolderSelected(
                        file.get_path()
                            .and_then(|p| p.into_os_string().into_string().ok())
                            .unwrap_or_else(|| "".to_string()),
                    ),
                    Ok(None) => Message::NoOp,
                    Err(err) => Message::FileError(err),
                }
            }),

            Message::FolderSelected(path) => {
                self.project_root = path;
                UpdateAction::Render
            }

            Message::ToggleWatch => {
                self.state = match self.state {
                    AppState::Watching => {
                        // stop the watcher (this may not actually stop the watcher)
                        self.watcher.take().unwrap().try_stop();

                        // get rid of the receiver
                        let context = MainContext::ref_thread_default();
                        let source = context
                            .find_source_by_id(&self.receiver_id.take().unwrap())
                            .unwrap();
                        source.destroy();

                        // clear output
                        self.results = None;

                        AppState::Idle
                    }

                    AppState::Idle => {
                        let (sender, receiver) = MainContext::channel(Default::default());
                        self.watcher = {
                            let mut watcher =
                                Watcher::new(&self.project_root, &self.command, sender)
                                    .expect("Failed to create watcher.");

                            watcher.start();

                            Some(watcher)
                        };

                        let results = self.results.clone();
                        self.receiver_id = Some(receiver.attach(None, move |result| {
                            // add the results to UI
                            println!("{}", result);
                            *results.as_ref().unwrap().borrow_mut() = result;

                            Continue(true)
                        }));

                        AppState::Watching
                    }
                };
                UpdateAction::Render
            }

            Message::PathChanged(path) => {
                self.project_root = path;
                UpdateAction::None
            }

            Message::CommandChanged(command) => {
                self.command = command;
                UpdateAction::None
            }

            Message::ClearOutput => {
                // self.results = None;
                UpdateAction::Render
            }

            Message::Exit => {
                vgtk::quit();
                UpdateAction::None
            }
        }
    }

    fn view(&self) -> VNode<Model> {
        gtk! {
            <Application::new_unwrap(Some("in.nerdworks.watch-rust-errors"), ApplicationFlags::empty())>

                <SimpleAction::new("quit", None) Application::accels=["<Ctrl>q"].as_ref() enabled=true
                        on activate=|a, _| Message::Exit/>

                <ApplicationWindow default_width=800 default_height=480 border_width=20 on destroy=|_| Message::Exit>
                    <HeaderBar title="Watch Rust Errors" show_close_button=true />
                    <Grid row_spacing=10 column_spacing=10>
                        // Row 0
                        <Label label="Project Root:" halign=Align::End />
                        <Entry Grid::left=1 hexpand=true
                               editable={ self.state.map(|| true, || false) }
                               text=self.project_root.clone()
                               on property_text_notify=|inp| {
                                   match inp.get_text().map(|s| s.as_str().to_owned()) {
                                       Some(path) => Message::PathChanged(path),
                                       None => Message::NoOp,
                                   }
                                } />
                        <Button label="..."
                                Grid::left=2
                                sensitive={ self.state.map(|| true, || false) }
                                on clicked=|_| Message::SelectFolder />

                        // Row 1
                        <Label label="Command:" halign=Align::End Grid::top=1 />
                        <Entry Grid::left=1 Grid::top=1
                               hexpand=true
                               editable={ self.state.map(|| true, || false) }
                               text=self.command.clone()
                               placeholder_text="cargo check"
                               on property_text_notify=|inp| {
                                   match inp.get_text().map(|s| s.as_str().to_owned()) {
                                       Some(command) => Message::CommandChanged(command),
                                       None => Message::NoOp,
                                   }
                               } />
                        <Button label={ self.state.map(|| "Start Watching", || "Stop Watching") }
                            Grid::left=2
                            Grid::top=1
                            on clicked=|button| Message::ToggleWatch />

                        // Row 2
                        <ScrolledWindow Grid::top=2 Grid::width=3 hexpand=true vexpand=true>
                            <ListBox selection_mode=SelectionMode::None>
                               {
                                   self.render_results()
                               }
                            </ListBox>
                        </ScrolledWindow>

                        // Row 3
                        <Button label="Clear Output"
                            Grid::left=2
                            Grid::top=3
                            on clicked=|_| Message::ClearOutput />
                    </Grid>
                </ApplicationWindow>
            </Application>
        }
    }
}

async fn select_folder() -> Result<Option<File>, Error> {
    let dialog = FileChooserNative::new(
        Some("Select root folder of your crate"),
        vgtk::current_object()
            .and_then(|w| w.downcast::<Window>().ok())
            .as_ref(),
        FileChooserAction::SelectFolder,
        Some("Select"),
        None,
    );
    dialog.set_modal(true);
    dialog.show();

    if on_signal!(dialog, connect_response).await == Ok(ResponseType::Accept) {
        Ok(dialog.get_file())
    } else {
        Ok(None)
    }
}

fn main() {
    std::process::exit(run::<Model>());
}
