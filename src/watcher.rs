use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};

use glib::Sender;
use watchexec::{
    error::{Error as WatchError, Result as WatchResult},
    pathop::PathOp,
    Args, ArgsBuilder, Handler,
};

use crate::cargo::{self, CompileResult};

struct State {
    project_root: PathBuf,
    command: String,
    quit: bool,
    tx: Sender<CompileResult>,
    runner: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub struct Watcher {
    state: Arc<RwLock<State>>,
}

impl Watcher {
    pub fn new<P: AsRef<Path>>(
        project_root: P,
        command: &str,
        tx: Sender<CompileResult>,
    ) -> Result<Self, String> {
        Ok(Watcher {
            state: Arc::new(RwLock::new(State {
                project_root: project_root.as_ref().to_path_buf(),
                command: command.to_string(),
                quit: false,
                tx,
                runner: None,
            })),
        })
    }

    pub fn start(&mut self) {
        let this = self.clone();
        self.state.write().unwrap().runner = Some(thread::spawn(move || {
            watchexec::watch(&this).unwrap();
        }));
    }

    pub fn try_stop(&mut self) {
        self.state.write().unwrap().quit = true;
    }

    fn run(&self) -> Result<CompileResult, String> {
        cargo::run(
            &self.state.read().unwrap().project_root,
            &self.state.read().unwrap().command,
        )
    }
}

impl Handler for Watcher {
    fn on_manual(&self) -> WatchResult<bool> {
        if self.state.read().unwrap().quit {
            return Ok(false);
        }

        self.run()
            .and_then(|results| {
                self.state
                    .read()
                    .unwrap()
                    .tx
                    .send(results)
                    .map_err(|e| format!("{:?}", e))
            })
            .map(|_| true)
            .map_err(|err| WatchError::Io(IoError::new(IoErrorKind::Other, format!("{:?}", err))))
    }

    fn on_update(&self, _ops: &[PathOp]) -> WatchResult<bool> {
        self.on_manual()
    }

    fn args(&self) -> Args {
        ArgsBuilder::default()
            .paths(vec![self.state.read().unwrap().project_root.clone()])
            .cmd(vec![self.state.read().unwrap().command.clone()])
            .filters(vec!["**/*.toml".to_owned(), "**/*.rs".to_owned()])
            .debounce(500_u64)
            .run_initially(true)
            .build()
            .unwrap()
    }
}
