use crate::{
    error::{wrap_err, NeomacsError, Result},
    state::{State, StateManager},
};
use std::{
    collections::{BTreeMap, HashMap},
    iter,
    sync::Arc,
};

use async_trait::async_trait;
use log::error;
use tokio::sync::{mpsc, oneshot, Mutex};

/// Data about the context in which a command was called
// TODO figure out what else should go in here (keybinding used to call, etc)
#[derive(Debug)]
pub struct CommandContext {
    pub name: String,
}

#[derive(Debug, PartialEq)]
pub enum Type {
    Nil,
    Boolean,
    Integer,
    Float,
    String,
    Optional,
    List,
    Map,
}

#[derive(Debug, Clone)]
pub enum Value {
    Nil,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Optional(Option<Box<Value>>),
    List(Vec<Value>),
    Map(HashMap<Value, Value>),
}

impl Value {
    pub fn get_type(&self) -> Type {
        match self {
            Value::Nil => Type::Nil,
            Value::Boolean(_) => Type::Boolean,
            Value::Integer(_) => Type::Integer,
            Value::Float(_) => Type::Float,
            Value::String(_) => Type::String,
            Value::Optional(_) => Type::Optional,
            Value::List(_) => Type::List,
            Value::Map(_) => Type::Map,
        }
    }
}

pub struct Command {
    name: String,
    param_types: Vec<Type>,
    return_type: Type,
}

#[async_trait]
pub trait CommandHandler {
    const SIGNATURE: (&'static [Type], Type);
    const NAME: &'static str;
    async fn compute_inputs(&self, state: &State, ctx: &CommandContext) -> Result<Vec<Value>>;
    async fn execute(&self, state: StateManager<State>, input: Vec<Value>) -> Result<Value>;
}

pub type CommandInteractiveMessage = (
    StateManager<State>,
    CommandContext,
    oneshot::Sender<Result<Value>>,
);
pub type CommandWithParamsMessage = (
    StateManager<State>,
    CommandContext,
    Vec<Value>,
    oneshot::Sender<Result<Value>>,
);

pub struct CommandService<H: CommandHandler + Send + Sync + 'static> {
    handler: Arc<Mutex<H>>,
    interactive_tx: mpsc::Sender<CommandInteractiveMessage>,
    interactive_rx: Arc<Mutex<mpsc::Receiver<CommandInteractiveMessage>>>,
    with_params_tx: mpsc::Sender<CommandWithParamsMessage>,
    with_params_rx: Arc<Mutex<mpsc::Receiver<CommandWithParamsMessage>>>,
}

impl<H: CommandHandler + Send + Sync + 'static> CommandService<H> {
    pub fn new(handler: H) -> Self {
        let (interactive_tx, interactive_rx) = mpsc::channel(256);
        let (with_params_tx, with_params_rx) = mpsc::channel(256);
        Self {
            handler: Arc::new(Mutex::new(handler)),
            interactive_tx,
            interactive_rx: Arc::new(Mutex::new(interactive_rx)),
            with_params_tx,
            with_params_rx: Arc::new(Mutex::new(with_params_rx)),
        }
    }

    /// Returns a handle that can be used to execute the command interactively
    pub fn interactive_handle(&self) -> mpsc::Sender<CommandInteractiveMessage> {
        self.interactive_tx.clone()
    }

    /// Returns a handle that can be used to execute the command with specified params
    pub fn with_params_handle(&self) -> mpsc::Sender<CommandWithParamsMessage> {
        self.with_params_tx.clone()
    }

    pub fn start(&self) {
        let mut interactive_rx = self.interactive_rx.clone();
        let handler = self.handler.clone();
        tokio::spawn(async move {
            while let Some((state, ctx, result_tx)) =
                Self::next_interactive_input(&mut interactive_rx).await
            {
                let params = {
                    let handler = handler.lock().await;
                    handler.compute_inputs(&state.snapshot(), &ctx).await
                };
                if let Err(e) = params {
                    error!("Error computing params: {}", e);
                    continue;
                }
                let result = match Self::validate_command_input(params.as_ref().unwrap()) {
                    Err(e) => Err(e),
                    Ok(_) => {
                        let handler = handler.lock().await;
                        handler.execute(state, params.unwrap()).await
                    }
                };
                if result_tx.send(result).is_err() {
                    error!("Error sending command result for command {}", ctx.name);
                }
            }
        });
        let mut with_params_rx = self.with_params_rx.clone();
        let handler = self.handler.clone();
        tokio::spawn(async move {
            while let Some((state, ctx, params, result_tx)) =
                Self::next_with_params_input(&mut with_params_rx).await
            {
                let result = match Self::validate_command_input(&params) {
                    Err(e) => Err(e),
                    Ok(_) => {
                        let handler = handler.lock().await;
                        handler.execute(state, params).await
                    }
                };
                if result_tx.send(result).is_err() {
                    error!("Error sending command result for command {}", ctx.name);
                }
            }
        });
    }

    fn validate_command_input(input: &Vec<Value>) -> Result<()> {
        let expected_types = H::SIGNATURE.0;
        if expected_types.len() != input.len() {
            return Err(NeomacsError::invalid_command_input(
                H::NAME,
                expected_types,
                input.to_vec(),
            ));
        }
        for (expected, actual) in iter::zip(expected_types.iter(), input.iter()) {
            if &actual.get_type() != expected {
                return Err(NeomacsError::invalid_command_input(
                    H::NAME,
                    expected_types,
                    input.to_vec(),
                ));
            }
        }
        Ok(())
    }

    async fn next_interactive_input(
        with_params_rx: &mut Arc<Mutex<mpsc::Receiver<CommandInteractiveMessage>>>,
    ) -> Option<CommandInteractiveMessage> {
        with_params_rx.lock().await.recv().await
    }

    async fn next_with_params_input(
        with_params_rx: &mut Arc<Mutex<mpsc::Receiver<CommandWithParamsMessage>>>,
    ) -> Option<CommandWithParamsMessage> {
        with_params_rx.lock().await.recv().await
    }
}

pub struct CommandDispatcher {
    state: StateManager<State>,
    registry: BTreeMap<
        String,
        (
            mpsc::Sender<CommandInteractiveMessage>,
            mpsc::Sender<CommandWithParamsMessage>,
        ),
    >,
}

impl CommandDispatcher {
    pub fn register_command<H: CommandHandler + Send + Sync>(
        &mut self,
        name: &str,
        command: CommandService<H>,
    ) {
        self.registry.insert(
            name.to_string(),
            (command.interactive_handle(), command.with_params_handle()),
        );
    }

    /// Executes a command "interactively", that is, computing the
    /// params from editor state or user input.
    pub async fn execute_interactively(&self, name: &str) -> Result<Value> {
        let handle = self
            .get_interactive_handle(name)
            .ok_or(NeomacsError::DoesNotExist(format!("Command {}", name)))?;
        let (tx, rx) = oneshot::channel();
        wrap_err(
            handle
                .send((self.state.clone(), Self::make_context(name), tx))
                .await,
        )?;
        wrap_err(rx.await)?
    }

    /// Executes a command with the passed-in input.
    pub async fn execute_command(&self, name: &str, input: Vec<Value>) -> Result<Value> {
        let handle = self
            .get_with_params_handle(name)
            .ok_or(NeomacsError::DoesNotExist(format!("Command {}", name)))?;
        let (tx, rx) = oneshot::channel();
        wrap_err(
            handle
                .send((self.state.clone(), Self::make_context(name), input, tx))
                .await,
        )?;
        wrap_err(rx.await)?
    }

    fn get_interactive_handle(
        &self,
        name: &str,
    ) -> Option<mpsc::Sender<CommandInteractiveMessage>> {
        self.registry
            .get(&name.to_string())
            .map(|(handle, _)| handle)
            .cloned()
    }

    fn get_with_params_handle(&self, name: &str) -> Option<mpsc::Sender<CommandWithParamsMessage>> {
        self.registry
            .get(&name.to_string())
            .map(|(_, handle)| handle)
            .cloned()
    }

    fn make_context(name: &str) -> CommandContext {
        CommandContext {
            name: name.to_string(),
        }
    }
}
