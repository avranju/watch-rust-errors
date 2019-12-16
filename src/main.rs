#![recursion_limit = "512"]

use std::str::FromStr;

use regex::Regex;
use vgtk::grid::GridProps;
use vgtk::lib::gio::{ActionExt, ApplicationFlags, File, FileExt, SimpleAction};
use vgtk::lib::glib::Error;
use vgtk::lib::gtk::{
    prelude::*, Align, Application, ApplicationWindow, Button, ButtonsType, DialogFlags, Entry,
    FileChooserAction, FileChooserNative, Grid, HeaderBar, Label, ListBox, MessageType,
    ResponseType, ScrolledWindow, SelectionMode, Window,
};
use vgtk::{ext::*, gtk, on_signal, run, Component, UpdateAction, VNode};

#[derive(Clone, Debug, Default)]
struct RustError {
    num: String,
    message: String,
    file: String,
    line: u32,
    column: u32,
    details: String,
}

impl RustError {
    fn new(num: &str, message: &str, file: &str, line: u32, column: u32, details: &str) -> Self {
        RustError {
            num: num.to_owned(),
            message: message.to_owned(),
            file: file.to_owned(),
            line,
            column,
            details: details.to_owned(),
        }
    }
}

impl FromStr for RustError {
    type Err = String;

    fn from_str(inp: &str) -> Result<Self, Self::Err> {
        // split input into 3 lines delimited by \n
        let lines: Vec<&str> = inp.splitn(3, '\n').collect();

        // extract error number and message
        let err = Regex::new(r"error\[E([0-9]+)]: (.*)")
            .unwrap()
            .captures(lines[0])
            .unwrap();
        let err_num = err.get(1).unwrap();
        let err_msg = err.get(2).unwrap();

        // extract file, line and col
        let context = Regex::new(r" +--> ([^:]+):([0-9]+):([0-9]+)")
            .unwrap()
            .captures(lines[1])
            .unwrap();
        let file = context.get(1).unwrap();
        let line = context.get(2).unwrap();
        let col = context.get(3).unwrap();

        Ok(RustError::new(
            err_num.as_str(),
            err_msg.as_str(),
            file.as_str(),
            line.as_str().parse().unwrap(),
            col.as_str().parse().unwrap(),
            lines[2],
        ))
    }
}

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

#[derive(Clone, Debug, Default)]
struct Model {
    root_folder: Option<File>,
    errors: Vec<RustError>,
    state: AppState,
}

#[derive(Clone, Debug)]
enum Message {
    NoOp,
    FolderSelected(File),
    SelectFolder,
    FileError(Error),
    ToggleWatch,
    Exit,
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
                    Ok(Some(file)) => Message::FolderSelected(file),
                    Ok(None) => Message::NoOp,
                    Err(err) => Message::FileError(err),
                }
            }),
            Message::FolderSelected(file) => {
                self.root_folder = Some(file);
                UpdateAction::Render
            }
            Message::ToggleWatch => {
                self.state = match self.state {
                    AppState::Watching => AppState::Idle,
                    AppState::Idle => AppState::Watching,
                };
                UpdateAction::Render
            },
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
                        <Entry Grid::left=1 hexpand=true text=self
                            .root_folder
                            .as_ref()
                            .and_then(|f| f.get_path())
                            .and_then(|p| p.into_os_string().into_string().ok())
                            .unwrap_or_else(|| "".to_string()) />
                        <Button label="..." Grid::left=2 on clicked=|_| Message::SelectFolder />

                        // Row 1
                        <Label label="Command:" halign=Align::End Grid::top=1 />
                        <Entry Grid::left=1 Grid::top=1 hexpand=true />
                        <Button label={
                            match self.state {
                                AppState::Idle => "Start Watching",
                                AppState::Watching => "Stop Watching",
                            }}
                            Grid::left=2
                            Grid::top=1
                            on clicked=|button| Message::ToggleWatch />

                        // Row 2
                        <ScrolledWindow Grid::top=2 Grid::width=3 hexpand=true vexpand=true>
                            <ListBox selection_mode=SelectionMode::None>
                            </ListBox>
                        </ScrolledWindow>
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
