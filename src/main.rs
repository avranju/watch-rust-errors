#![recursion_limit = "512"]

use std::str::FromStr;

use regex::Regex;
use vgtk::lib::gio::{ApplicationFlags, File, FileExt};
use vgtk::lib::glib::object::IsA;
use vgtk::lib::glib::Error;
use vgtk::lib::gtk::{
    prelude::*, Align, Application, ApplicationWindow, Button, ButtonsType, DialogFlags, Entry,
    FileChooserAction, FileChooserNative, Grid, HeaderBar, Label, ListBox, MessageType,
    ResponseType, ScrolledWindow, SelectionMode, Widget, Window,
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

#[derive(Clone, Debug, Default)]
struct Model {
    root_folder: Option<File>,
    errors: Vec<RustError>,
}

#[derive(Clone, Debug)]
enum Message {
    NoOp,
    FolderSelected(File),
    SelectFolder,
    FileError(Error),
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
            Message::Exit => {
                vgtk::quit();
                UpdateAction::None
            }
        }
    }

    fn view(&self) -> VNode<Model> {
        gtk! {
            <Application::new_unwrap(Some("in.nerdworks.watch-rust-errors"), ApplicationFlags::empty())>
                <ApplicationWindow default_width=800 default_height=480 border_width=20 on destroy=|_| Message::Exit>
                    <HeaderBar title="Watch Rust Errors" show_close_button=true />
                    <Grid row_spacing=10 column_spacing=10>
                        // Row 0
                        <Label label="Project Root:" Grid::position=GridPosition::default() />
                        <Entry Grid::position=GridPosition { left: 1, ..Default::default() }
                               hexpand=true text=self
                                    .root_folder
                                    .as_ref()
                                    .and_then(|f| f.get_path())
                                    .and_then(|p| p.into_os_string().into_string().ok())
                                    .unwrap_or_else(|| "".to_string()) />
                        <Button label="..." Grid::position=GridPosition { left: 2, ..Default::default() }
                            on clicked=|_| Message::SelectFolder />

                        // Row 1
                        <Label label="Command:" halign=Align::End Grid::position=GridPosition { top: 1, ..Default::default() } />
                        <Entry Grid::position=GridPosition { left: 1, top: 1, ..Default::default() } hexpand=true />
                        <Button label="Start Watching" Grid::position=GridPosition { left: 2, top: 1, ..Default::default() } />

                        // Row 2
                        <ScrolledWindow Grid::position=GridPosition { top: 2, width: 3, ..Default::default() } hexpand=true vexpand=true>
                            <ListBox selection_mode=SelectionMode::None>
                            </ListBox>
                        </ScrolledWindow>
                    </Grid>
                </ApplicationWindow>
            </Application>
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct GridPosition {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

impl Default for GridPosition {
    fn default() -> Self {
        GridPosition {
            left: 0,
            top: 0,
            width: 1,
            height: 1,
        }
    }
}

trait GridProps {
    fn set_child_position<P: IsA<Widget>>(&self, child: &P, position: GridPosition);
    fn get_child_position<P: IsA<Widget>>(&self, child: &P) -> GridPosition;
}

impl GridProps for Grid {
    fn set_child_position<P: IsA<Widget>>(&self, child: &P, position: GridPosition) {
        self.set_cell_left_attach(child, position.left);
        self.set_cell_top_attach(child, position.top);
        self.set_cell_width(child, position.width);
        self.set_cell_height(child, position.height);
    }

    fn get_child_position<P: IsA<Widget>>(&self, child: &P) -> GridPosition {
        GridPosition {
            left: self.get_cell_left_attach(child),
            top: self.get_cell_top_attach(child),
            width: self.get_cell_width(child),
            height: self.get_cell_height(child),
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
