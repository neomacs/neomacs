use crate::{
    error::{wrap_err, NeomacsError, Result},
    rpc::{
        codec::{Request, Response},
        convert::{DecodeValue, EncodeValue},
        extensions::{decode_optional, encode_optional, is_optional},
        handler::{RequestContext, RequestHandler},
        server::ClientComms,
    },
    state::{State, StateManager},
};
use std::{collections::BTreeMap, iter, ops::Deref, sync::Arc};

use async_trait::async_trait;
use log::error;
use neomacs_proc_macros::{DecodeValue, EncodeValue};
use ordered_float::OrderedFloat;
use tokio::sync::{mpsc, oneshot, Mutex};

const BUILT_IN_NAMESPACE: &str = "neomacs";

/// Data about the context in which a command was called
// TODO figure out what else should go in here (keybinding used to call, etc)
#[derive(Clone, Debug, EncodeValue)]
pub struct CommandContext {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    Nil,
    Boolean,
    Integer,
    Float,
    String,
    // TODO make optional have a specific inner type
    Optional,
    List,
    Map,
}

impl DecodeValue for Type {
    fn decode_value(value: &rmpv::Value) -> Result<Self> {
        match value
            .as_str()
            .ok_or_else(|| NeomacsError::MessagePackParse(format!("Invalid Type: {}", value)))?
        {
            "Nil" => Ok(Type::Nil),
            "Boolean" => Ok(Type::Boolean),
            "Integer" => Ok(Type::Integer),
            "Float" => Ok(Type::Float),
            "String" => Ok(Type::String),
            "Optional" => Ok(Type::Optional),
            "List" => Ok(Type::List),
            "Map" => Ok(Type::Map),
            _ => Err(NeomacsError::MessagePackParse(format!(
                "Invalid Type: {}",
                value
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    Nil,
    Boolean(bool),
    Integer(i64),
    Float(OrderedFloat<f64>),
    String(String),
    Optional(Option<Box<Value>>),
    List(Vec<Value>),
    Map(Vec<(Value, Value)>),
}

impl EncodeValue for Value {
    fn encode_value(&self) -> rmpv::Value {
        match self {
            Value::Nil => rmpv::Value::Nil,
            Value::Boolean(v) => rmpv::Value::Boolean(*v),
            Value::Integer(v) => rmpv::Value::Integer(rmpv::Integer::from(*v)),
            Value::Float(v) => rmpv::Value::F64(*v.deref()),
            Value::String(v) => rmpv::Value::String(rmpv::Utf8String::from(v.as_str())),
            Value::Optional(v) => encode_optional(&v.as_deref().cloned()),
            Value::List(v) => rmpv::Value::Array(v.iter().map(|val| val.encode_value()).collect()),
            Value::Map(v) => rmpv::Value::Map(
                v.iter()
                    .map(|(key, val)| (key.encode_value(), val.encode_value()))
                    .collect(),
            ),
        }
    }
}

impl DecodeValue for Value {
    fn decode_value(value: &rmpv::Value) -> Result<Self> {
        match value {
            rmpv::Value::Nil => Ok(Value::Nil),
            rmpv::Value::Boolean(v) => Ok(Value::Boolean(*v)),
            rmpv::Value::Integer(v) => Ok(Value::Integer(v.as_i64().unwrap())),
            rmpv::Value::F32(v) => Ok(Value::Float((*v as f64).into())),
            rmpv::Value::F64(v) => Ok(Value::Float(OrderedFloat::from(*v))),
            rmpv::Value::String(v) => Ok(Value::String(v.clone().into_str().unwrap())),
            rmpv::Value::Array(v) => {
                let mut arr = Vec::new();
                for val in v {
                    arr.push(Self::decode_value(val)?);
                }
                Ok(Value::List(arr))
            }
            rmpv::Value::Map(v) => {
                let mut map = Vec::new();
                for (key, val) in v {
                    map.push((Self::decode_value(key)?, Self::decode_value(val)?));
                }
                Ok(Value::Map(map))
            }
            rmpv::Value::Ext(_, _) if is_optional(&value) => {
                decode_optional(&value).map(|val| Value::Optional(val.map(Box::new)))
            }
            _ => Err(NeomacsError::MessagePackParse(format!(
                "Invalid command input value: {}",
                value
            ))),
        }
    }
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

#[async_trait]
pub trait CommandHandler {
    const SIGNATURE: (&'static [Type], Type);
    const NAME: &'static str;
    async fn compute_inputs(&self, state: &State, ctx: &CommandContext) -> Result<Vec<Value>>;
    async fn invoke(&self, state: StateManager<State>, input: Vec<Value>) -> Result<Value>;
}

pub type CommandInvokeInteractivelyMessage = (
    StateManager<State>,
    CommandContext,
    oneshot::Sender<Result<Value>>,
);
pub type CommandInvokeMessage = (
    StateManager<State>,
    CommandContext,
    Vec<Value>,
    oneshot::Sender<Result<Value>>,
);

pub struct BuiltinCommandManager<H: CommandHandler + Send + Sync + 'static> {
    handler: Arc<Mutex<H>>,
    invoke_interactively_tx: mpsc::Sender<CommandInvokeInteractivelyMessage>,
    invoke_interactively_rx: Arc<Mutex<mpsc::Receiver<CommandInvokeInteractivelyMessage>>>,
    invoke_tx: mpsc::Sender<CommandInvokeMessage>,
    invoke_rx: Arc<Mutex<mpsc::Receiver<CommandInvokeMessage>>>,
}

impl<H: CommandHandler + Send + Sync + 'static> BuiltinCommandManager<H> {
    pub fn new(handler: H) -> Self {
        let (invoke_interactively_tx, invoke_interactively_rx) = mpsc::channel(256);
        let (invoke_tx, invoke_rx) = mpsc::channel(256);
        Self {
            handler: Arc::new(Mutex::new(handler)),
            invoke_interactively_tx,
            invoke_interactively_rx: Arc::new(Mutex::new(invoke_interactively_rx)),
            invoke_tx,
            invoke_rx: Arc::new(Mutex::new(invoke_rx)),
        }
    }

    /// Returns a handle that can be used to invoke the command interactively
    pub fn invoke_interactively_handle(&self) -> mpsc::Sender<CommandInvokeInteractivelyMessage> {
        self.invoke_interactively_tx.clone()
    }

    /// Returns a handle that can be used to invoke the command with specified params
    pub fn invoke_handle(&self) -> mpsc::Sender<CommandInvokeMessage> {
        self.invoke_tx.clone()
    }

    pub fn start(&self) {
        let mut interactive_rx = self.invoke_interactively_rx.clone();
        let handler = self.handler.clone();
        tokio::spawn(async move {
            while let Some((state, ctx, result_tx)) =
                Self::next_invoke_interactively_input(&mut interactive_rx).await
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
                        handler.invoke(state, params.unwrap()).await
                    }
                };
                if result_tx.send(result).is_err() {
                    error!("Error sending command result for command {}", ctx.name);
                }
            }
        });
        let mut invoke_rx = self.invoke_rx.clone();
        let handler = self.handler.clone();
        tokio::spawn(async move {
            while let Some((state, ctx, params, result_tx)) =
                Self::next_invoke_input(&mut invoke_rx).await
            {
                let result = match Self::validate_command_input(&params) {
                    Err(e) => Err(e),
                    Ok(_) => {
                        let handler = handler.lock().await;
                        handler.invoke(state, params).await
                    }
                };
                if result_tx.send(result).is_err() {
                    error!("Error sending command result for command {}", ctx.name);
                }
            }
        });
    }

    fn validate_command_input(input: &[Value]) -> Result<()> {
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

    async fn next_invoke_interactively_input(
        invoke_interactively_rx: &mut Arc<Mutex<mpsc::Receiver<CommandInvokeInteractivelyMessage>>>,
    ) -> Option<CommandInvokeInteractivelyMessage> {
        invoke_interactively_rx.lock().await.recv().await
    }

    async fn next_invoke_input(
        invoke_rx: &mut Arc<Mutex<mpsc::Receiver<CommandInvokeMessage>>>,
    ) -> Option<CommandInvokeMessage> {
        invoke_rx.lock().await.recv().await
    }
}

#[derive(Clone, DecodeValue)]
struct ExternalCommand {
    namespace: String,
    name: String,
    param_types: Vec<Type>,
    return_type: Type,
}

#[derive(EncodeValue)]
struct ComputeInputsRequest {
    command: String,
    context: CommandContext,
}

#[derive(DecodeValue)]
struct ComputeInputsResponse {
    params: Vec<Value>,
}

#[derive(EncodeValue)]
struct InvokeRequest {
    command: String,
    params: Vec<Value>,
}

struct ExternalCommandManager {
    comms: ClientComms,
    client_id: u64,
    command: ExternalCommand,
    invoke_interactively_tx: mpsc::Sender<CommandInvokeInteractivelyMessage>,
    invoke_interactively_rx: Arc<Mutex<mpsc::Receiver<CommandInvokeInteractivelyMessage>>>,
    invoke_tx: mpsc::Sender<CommandInvokeMessage>,
    invoke_rx: Arc<Mutex<mpsc::Receiver<CommandInvokeMessage>>>,
}

impl ExternalCommandManager {
    fn new(comms: ClientComms, client_id: u64, command: ExternalCommand) -> Self {
        let (invoke_interactively_tx, invoke_interactively_rx) = mpsc::channel(256);
        let (invoke_tx, invoke_rx) = mpsc::channel(256);
        Self {
            comms,
            client_id,
            command,
            invoke_interactively_tx,
            invoke_interactively_rx: Arc::new(Mutex::new(invoke_interactively_rx)),
            invoke_tx,
            invoke_rx: Arc::new(Mutex::new(invoke_rx)),
        }
    }

    /// Returns a handle that can be used to invoke the command interactively
    pub fn invoke_interactively_handle(&self) -> mpsc::Sender<CommandInvokeInteractivelyMessage> {
        self.invoke_interactively_tx.clone()
    }

    /// Returns a handle that can be used to invoke the command with specified params
    pub fn invoke_handle(&self) -> mpsc::Sender<CommandInvokeMessage> {
        self.invoke_tx.clone()
    }

    pub fn start(&self) {
        let mut interactive_rx = self.invoke_interactively_rx.clone();
        let command = self.command.clone();
        let mut comms = self.comms.clone();
        let client_id = self.client_id;
        tokio::spawn(async move {
            while let Some((_state, ctx, result_tx)) =
                Self::next_invoke_interactively_input(&mut interactive_rx).await
            {
                let params = comms
                    .request(
                        client_id,
                        "compute_inputs",
                        &[ComputeInputsRequest {
                            command: ctx.name.clone(),
                            context: ctx.clone(),
                        }
                        .encode_value()],
                    )
                    .await
                    .and_then(|res| res.try_unwrap())
                    .and_then(|val| ComputeInputsResponse::decode_value(&val));
                if let Err(e) = params {
                    // TODO once there's an error handling system, send this error there.
                    error!("Error computing params: {}", e);
                    continue;
                }
                let result = match Self::validate_command_input(
                    &command,
                    &params.as_ref().unwrap().params,
                ) {
                    Err(e) => Err(e),
                    Ok(_) => {
                        comms
                            .request(
                                client_id,
                                "invoke",
                                &[InvokeRequest {
                                    command: ctx.name.clone(),
                                    params: params.unwrap().params,
                                }
                                .encode_value()],
                            )
                            .await
                    }
                }
                .and_then(|res| res.try_unwrap())
                .and_then(|res| Value::decode_value(&res));
                // TODO once there's an error handling system, if result is an error send it there
                if result_tx.send(result).is_err() {
                    error!("Error sending command result for command {}", ctx.name);
                }
            }
        });
        let mut invoke_rx = self.invoke_rx.clone();
        let command = self.command.clone();
        let mut comms = self.comms.clone();
        tokio::spawn(async move {
            while let Some((_state, ctx, params, result_tx)) =
                Self::next_invoke_input(&mut invoke_rx).await
            {
                let result = match Self::validate_command_input(&command, &params) {
                    Err(e) => Err(e),
                    Ok(_) => {
                        comms
                            .request(
                                client_id,
                                "invoke",
                                &[InvokeRequest {
                                    command: ctx.name.clone(),
                                    params,
                                }
                                .encode_value()],
                            )
                            .await
                    }
                }
                .and_then(|res| res.try_unwrap())
                .and_then(|res| Value::decode_value(&res));
                // TODO once there's an error handling system, if result is an error send it there
                if result_tx.send(result).is_err() {
                    error!("Error sending command result for command {}", ctx.name);
                }
            }
        });
    }

    fn validate_command_input(cmd: &ExternalCommand, input: &[Value]) -> Result<()> {
        let expected_types = &cmd.param_types;
        if expected_types.len() != input.len() {
            return Err(NeomacsError::invalid_command_input(
                cmd.name.as_str(),
                expected_types,
                input.to_vec(),
            ));
        }
        for (expected, actual) in iter::zip(expected_types.iter(), input.iter()) {
            if &actual.get_type() != expected {
                return Err(NeomacsError::invalid_command_input(
                    cmd.name.as_str(),
                    expected_types,
                    input.to_vec(),
                ));
            }
        }
        Ok(())
    }

    async fn next_invoke_interactively_input(
        invoke_interactively_rx: &mut Arc<Mutex<mpsc::Receiver<CommandInvokeInteractivelyMessage>>>,
    ) -> Option<CommandInvokeInteractivelyMessage> {
        invoke_interactively_rx.lock().await.recv().await
    }

    async fn next_invoke_input(
        invoke_rx: &mut Arc<Mutex<mpsc::Receiver<CommandInvokeMessage>>>,
    ) -> Option<CommandInvokeMessage> {
        invoke_rx.lock().await.recv().await
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
struct DispatcherKey {
    namespace: String,
    name: String,
}

impl DispatcherKey {
    fn new<S1: Into<String>, S2: Into<String>>(namespace: S1, name: S2) -> Self {
        Self {
            namespace: namespace.into(),
            name: name.into(),
        }
    }
}

pub struct CommandDispatcher {
    state: StateManager<State>,
    comms: ClientComms,
    registry: BTreeMap<
        DispatcherKey,
        (
            mpsc::Sender<CommandInvokeInteractivelyMessage>,
            mpsc::Sender<CommandInvokeMessage>,
        ),
    >,
}

impl CommandDispatcher {
    pub fn new(state: StateManager<State>, comms: ClientComms) -> Self {
        Self {
            state,
            comms,
            registry: BTreeMap::new(),
        }
    }

    pub fn register_builtin_command<H: CommandHandler + Send + Sync>(
        &mut self,
        name: &str,
        command: BuiltinCommandManager<H>,
    ) {
        self.registry.insert(
            DispatcherKey::new(BUILT_IN_NAMESPACE, name),
            (
                command.invoke_interactively_handle(),
                command.invoke_handle(),
            ),
        );
    }

    fn register_external_command(&mut self, cmd_mgr: &ExternalCommandManager) {
        self.registry.insert(
            DispatcherKey::new(
                cmd_mgr.command.namespace.as_str(),
                cmd_mgr.command.name.as_str(),
            ),
            (
                cmd_mgr.invoke_interactively_handle(),
                cmd_mgr.invoke_handle(),
            ),
        );
    }

    /// Invokes a command "interactively", that is, computing the
    /// params from editor state or user input.
    pub async fn invoke_interactively(&self, namespace: &str, name: &str) -> Result<Value> {
        let handle = self
            .invoke_interactively_handle(namespace, name)
            .ok_or_else(|| NeomacsError::DoesNotExist(format!("Command {}", name)))?;
        let (tx, rx) = oneshot::channel();
        wrap_err(
            handle
                .send((self.state.clone(), Self::make_context(name), tx))
                .await,
        )?;
        wrap_err(rx.await)?
    }

    /// Invokes a command with the passed-in input.
    pub async fn invoke(&self, namespace: &str, name: &str, input: Vec<Value>) -> Result<Value> {
        let handle = self
            .invoke_handle(namespace, name)
            .ok_or_else(|| NeomacsError::DoesNotExist(format!("Command {}", name)))?;
        let (tx, rx) = oneshot::channel();
        wrap_err(
            handle
                .send((self.state.clone(), Self::make_context(name), input, tx))
                .await,
        )?;
        wrap_err(rx.await)?
    }

    fn invoke_interactively_handle(
        &self,
        namespace: &str,
        name: &str,
    ) -> Option<mpsc::Sender<CommandInvokeInteractivelyMessage>> {
        self.registry
            .get(&DispatcherKey::new(namespace, name))
            .map(|(handle, _)| handle)
            .cloned()
    }

    fn invoke_handle(
        &self,
        namespace: &str,
        name: &str,
    ) -> Option<mpsc::Sender<CommandInvokeMessage>> {
        self.registry
            .get(&DispatcherKey::new(namespace, name))
            .map(|(_, handle)| handle)
            .cloned()
    }

    fn make_context(name: &str) -> CommandContext {
        CommandContext {
            name: name.to_string(),
        }
    }

    async fn rpc_defcommand(&mut self, client_id: u64, request: &Request) -> Result<Response> {
        if request.params.len() != 1 {
            return Err(NeomacsError::InvalidRPCMessage);
        }
        let cmd = ExternalCommand::decode_value(&request.params[0])?;
        let mgr = ExternalCommandManager::new(self.comms.clone(), client_id, cmd);
        mgr.start();
        self.register_external_command(&mgr);
        Ok(Response::success_with_status(request.msg_id, "OK"))
    }

    async fn rpc_invoke(&self, request: &Request) -> Result<Response> {
        if request.params.len() != 1 {
            return Err(NeomacsError::InvalidRPCMessage);
        }
        let req = RpcInvokeRequest::decode_value(&request.params[0])?;
        let result = self
            .invoke(req.namespace.as_str(), req.name.as_str(), req.params)
            .await?;
        Ok(Response::success(
            request.msg_id,
            Some(result.encode_value()),
        ))
    }

    async fn rpc_invoke_interactively(&self, request: &Request) -> Result<Response> {
        if request.params.len() != 1 {
            return Err(NeomacsError::InvalidRPCMessage);
        }
        let req = RpcInvokeInteractivelyRequest::decode_value(&request.params[0])?;
        let result = self
            .invoke_interactively(req.namespace.as_str(), req.name.as_str())
            .await?;
        Ok(Response::success(
            request.msg_id,
            Some(result.encode_value()),
        ))
    }
}

#[derive(DecodeValue)]
struct RpcInvokeRequest {
    namespace: String,
    name: String,
    params: Vec<Value>,
}

#[derive(DecodeValue)]
struct RpcInvokeInteractivelyRequest {
    namespace: String,
    name: String,
}

#[async_trait]
impl RequestHandler for CommandDispatcher {
    fn handled_methods() -> Vec<&'static str> {
        vec!["defcommand", "invoke_cmd", "invoke_cmd_interactively"]
    }

    async fn handle(&mut self, context: RequestContext, request: &Request) -> Result<Response> {
        match request.method.as_str() {
            "defcommand" => self.rpc_defcommand(context.connection_id, request).await,
            "invoke" => self.rpc_invoke(request).await,
            "invoke_interactively" => self.rpc_invoke_interactively(request).await,
            _ => Err(NeomacsError::InvalidRPCMessage),
        }
    }
}
