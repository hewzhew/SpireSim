use crate::{
    EngineContractV1, EngineHelloV1, EngineOperationV1, EngineOutcomeV1, EngineRequestV1,
    EngineResponseV1, SupervisorCallOutcomeV1, SupervisorCallV1, SupervisorErrorV1,
    ENGINE_HELLO_SCHEMA_V1, ENGINE_RESPONSE_SCHEMA_V1,
};
use serde_json::Value;
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const MAX_ENGINE_FRAME_BYTES_V1: usize = 256 * 1024;
pub const MAX_SUPERVISOR_CALLS_PER_EXECUTE_V1: usize = 64;

#[derive(Debug, Eq, PartialEq)]
pub enum EngineProcessErrorV1 {
    InvalidContract(SupervisorErrorV1),
    Spawn(String),
    MissingPipe(&'static str),
    Deadline(&'static str),
    Disconnected,
    InvalidFrame(String),
    FrameTooLarge,
    ContractMismatch,
    AlreadyBound,
    NotBound,
    InvalidEngineEpoch,
    ResponseMismatch,
    UnexpectedOutcome,
    InvalidSupervisorCall,
    SupervisorCallLimit,
    SupervisorCallFailed(String),
    Rejected { code: String },
    ProcessExit(String),
}

impl fmt::Display for EngineProcessErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for EngineProcessErrorV1 {}

/// One serialized stdio connection to a replaceable semantic Engine.
///
/// The Supervisor treats Execute input and output as opaque typed values. It
/// owns framing, deadlines, process identity, request correlation, and epoch
/// binding; it never parses game scenes or action semantics.
pub struct EngineProcessV1 {
    child: Child,
    stdin: Option<ChildStdin>,
    frames: Receiver<Result<String, EngineProcessErrorV1>>,
    reader: Option<JoinHandle<()>>,
    contract: EngineContractV1,
    engine_epoch: Option<u64>,
    next_request: u64,
}

impl EngineProcessV1 {
    pub fn spawn(
        command: &mut Command,
        expected_contract: EngineContractV1,
        timeout: Duration,
    ) -> Result<Self, EngineProcessErrorV1> {
        expected_contract
            .validate()
            .map_err(EngineProcessErrorV1::InvalidContract)?;
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| EngineProcessErrorV1::Spawn(error.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or(EngineProcessErrorV1::MissingPipe("stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(EngineProcessErrorV1::MissingPipe("stdout"))?;
        let (sender, frames) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_bounded_line(&mut reader) {
                    Ok(Some(line)) => {
                        if sender.send(Ok(line)).is_err() {
                            return;
                        }
                    }
                    Ok(None) => return,
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        return;
                    }
                }
            }
        });
        let mut process = Self {
            child,
            stdin: Some(stdin),
            frames,
            reader: Some(reader),
            contract: expected_contract,
            engine_epoch: None,
            next_request: 1,
        };
        let hello = process.receive_hello(timeout)?;
        if hello.schema != ENGINE_HELLO_SCHEMA_V1 || hello.contract != process.contract {
            return Err(EngineProcessErrorV1::ContractMismatch);
        }
        Ok(process)
    }

    pub fn process_id(&self) -> u32 {
        self.child.id()
    }

    pub fn contract(&self) -> &EngineContractV1 {
        &self.contract
    }

    pub fn require_running(&mut self) -> Result<(), EngineProcessErrorV1> {
        match self.child.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => Err(EngineProcessErrorV1::ProcessExit(status.to_string())),
            Err(error) => Err(EngineProcessErrorV1::ProcessExit(error.to_string())),
        }
    }

    pub fn bind(
        &mut self,
        engine_epoch: u64,
        timeout: Duration,
    ) -> Result<(), EngineProcessErrorV1> {
        if self.engine_epoch.is_some() {
            return Err(EngineProcessErrorV1::AlreadyBound);
        }
        if engine_epoch == 0 {
            return Err(EngineProcessErrorV1::InvalidEngineEpoch);
        }
        let outcome = self.exchange_message(engine_epoch, EngineOperationV1::Bind, timeout)?;
        match outcome {
            EngineOutcomeV1::Bound => self.engine_epoch = Some(engine_epoch),
            EngineOutcomeV1::Rejected { code } => {
                return Err(EngineProcessErrorV1::Rejected { code });
            }
            _ => {
                self.terminate();
                return Err(EngineProcessErrorV1::UnexpectedOutcome);
            }
        }
        Ok(())
    }

    pub fn execute(
        &mut self,
        input: Value,
        timeout: Duration,
    ) -> Result<Value, EngineProcessErrorV1> {
        let engine_epoch = self.engine_epoch.ok_or(EngineProcessErrorV1::NotBound)?;
        match self.exchange_message(engine_epoch, EngineOperationV1::Execute { input }, timeout)? {
            EngineOutcomeV1::Completed { output } => Ok(output),
            EngineOutcomeV1::Rejected { code } => Err(EngineProcessErrorV1::Rejected { code }),
            _ => {
                self.terminate();
                Err(EngineProcessErrorV1::UnexpectedOutcome)
            }
        }
    }

    /// Executes one Engine job while servicing bounded, opaque calls back to
    /// the stable Supervisor. The callback owns domain-specific physical
    /// execution; this transport only enforces ordering, correlation, framing,
    /// and the maximum number of calls.
    pub fn execute_with_supervisor<Handle>(
        &mut self,
        input: Value,
        timeout: Duration,
        mut handle: Handle,
    ) -> Result<Value, EngineProcessErrorV1>
    where
        Handle: FnMut(SupervisorCallV1) -> Result<SupervisorCallOutcomeV1, String>,
    {
        let engine_epoch = self.engine_epoch.ok_or(EngineProcessErrorV1::NotBound)?;
        let mut outcome =
            self.exchange_message(engine_epoch, EngineOperationV1::Execute { input }, timeout)?;
        let mut call_count = 0_usize;
        let mut previous_call_ids = std::collections::BTreeSet::new();
        loop {
            match outcome {
                EngineOutcomeV1::Completed { output } => return Ok(output),
                EngineOutcomeV1::Rejected { code } => {
                    return Err(EngineProcessErrorV1::Rejected { code });
                }
                EngineOutcomeV1::SupervisorCall { call } => {
                    call_count += 1;
                    if call_count > MAX_SUPERVISOR_CALLS_PER_EXECUTE_V1 {
                        self.terminate();
                        return Err(EngineProcessErrorV1::SupervisorCallLimit);
                    }
                    if !valid_call_id(&call.call_id)
                        || !previous_call_ids.insert(call.call_id.clone())
                    {
                        self.terminate();
                        return Err(EngineProcessErrorV1::InvalidSupervisorCall);
                    }
                    let call_id = call.call_id.clone();
                    let callback_outcome = match handle(call) {
                        Ok(outcome) => outcome,
                        Err(error) => {
                            self.terminate();
                            return Err(EngineProcessErrorV1::SupervisorCallFailed(error));
                        }
                    };
                    if matches!(
                        &callback_outcome,
                        SupervisorCallOutcomeV1::Rejected { code } if !valid_error_code(code)
                    ) {
                        self.terminate();
                        return Err(EngineProcessErrorV1::InvalidSupervisorCall);
                    }
                    outcome = self.exchange_message(
                        engine_epoch,
                        EngineOperationV1::ResumeSupervisorCall {
                            call_id,
                            outcome: callback_outcome,
                        },
                        timeout,
                    )?;
                }
                _ => {
                    self.terminate();
                    return Err(EngineProcessErrorV1::UnexpectedOutcome);
                }
            }
        }
    }

    pub fn shutdown(mut self, timeout: Duration) -> Result<(), EngineProcessErrorV1> {
        let engine_epoch = self.engine_epoch.unwrap_or(0);
        let outcome = self.exchange_message(engine_epoch, EngineOperationV1::Shutdown, timeout)?;
        match outcome {
            EngineOutcomeV1::Stopped => {}
            EngineOutcomeV1::Rejected { code } => {
                return Err(EngineProcessErrorV1::Rejected { code });
            }
            _ => return Err(EngineProcessErrorV1::UnexpectedOutcome),
        }
        self.stdin.take();
        self.wait_for_exit(timeout)
    }

    fn receive_hello(&mut self, timeout: Duration) -> Result<EngineHelloV1, EngineProcessErrorV1> {
        let line = self.receive_line(timeout, "engine hello")?;
        serde_json::from_str(&line)
            .map_err(|error| EngineProcessErrorV1::InvalidFrame(error.to_string()))
    }

    fn exchange_message(
        &mut self,
        engine_epoch: u64,
        operation: EngineOperationV1,
        timeout: Duration,
    ) -> Result<EngineOutcomeV1, EngineProcessErrorV1> {
        let request_id = format!("supervisor-{engine_epoch}-{}", self.next_request);
        self.next_request += 1;
        let request = EngineRequestV1::new(&request_id, engine_epoch, operation);
        let encoded = serde_json::to_string(&request)
            .map_err(|error| EngineProcessErrorV1::InvalidFrame(error.to_string()))?;
        if encoded.len() > MAX_ENGINE_FRAME_BYTES_V1 {
            return Err(EngineProcessErrorV1::FrameTooLarge);
        }
        let stdin = self
            .stdin
            .as_mut()
            .ok_or(EngineProcessErrorV1::Disconnected)?;
        if let Err(error) = writeln!(stdin, "{encoded}").and_then(|_| stdin.flush()) {
            self.terminate();
            return Err(EngineProcessErrorV1::ProcessExit(error.to_string()));
        }

        let result = self.receive_response(timeout, &request_id, engine_epoch);
        if result.is_err() {
            self.terminate();
        }
        result
    }

    fn receive_response(
        &mut self,
        timeout: Duration,
        request_id: &str,
        engine_epoch: u64,
    ) -> Result<EngineOutcomeV1, EngineProcessErrorV1> {
        let line = self.receive_line(timeout, "engine response")?;
        let response: EngineResponseV1 = serde_json::from_str(&line)
            .map_err(|error| EngineProcessErrorV1::InvalidFrame(error.to_string()))?;
        if response.schema != ENGINE_RESPONSE_SCHEMA_V1
            || response.request_id != request_id
            || response.engine_epoch != engine_epoch
        {
            return Err(EngineProcessErrorV1::ResponseMismatch);
        }
        if let EngineOutcomeV1::Rejected { code } = &response.outcome {
            if !valid_error_code(code) {
                return Err(EngineProcessErrorV1::InvalidFrame(
                    "Engine rejection code is outside the closed wire vocabulary".to_owned(),
                ));
            }
        }
        Ok(response.outcome)
    }

    fn receive_line(
        &mut self,
        timeout: Duration,
        stage: &'static str,
    ) -> Result<String, EngineProcessErrorV1> {
        match self.frames.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(EngineProcessErrorV1::Deadline(stage)),
            Err(RecvTimeoutError::Disconnected) => {
                if let Ok(Some(status)) = self.child.try_wait() {
                    Err(EngineProcessErrorV1::ProcessExit(status.to_string()))
                } else {
                    Err(EngineProcessErrorV1::Disconnected)
                }
            }
        }
    }

    fn wait_for_exit(&mut self, timeout: Duration) -> Result<(), EngineProcessErrorV1> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) if status.success() => {
                    self.join_reader();
                    return Ok(());
                }
                Ok(Some(status)) => {
                    self.join_reader();
                    return Err(EngineProcessErrorV1::ProcessExit(status.to_string()));
                }
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(5));
                }
                Ok(None) => return Err(EngineProcessErrorV1::Deadline("engine shutdown")),
                Err(error) => return Err(EngineProcessErrorV1::ProcessExit(error.to_string())),
            }
        }
    }

    fn terminate(&mut self) {
        self.stdin.take();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        self.join_reader();
    }

    fn join_reader(&mut self) {
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for EngineProcessV1 {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn valid_error_code(code: &str) -> bool {
    let mut bytes = code.bytes();
    matches!(bytes.next(), Some(byte) if byte.is_ascii_alphabetic())
        && code.len() <= 64
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn valid_call_id(call_id: &str) -> bool {
    let mut bytes = call_id.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && call_id.len() <= 128
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

fn read_bounded_line<R: Read>(
    reader: &mut BufReader<R>,
) -> Result<Option<String>, EngineProcessErrorV1> {
    let mut bytes = Vec::new();
    loop {
        let (consumed, found_newline) = {
            let available = reader
                .fill_buf()
                .map_err(|error| EngineProcessErrorV1::ProcessExit(error.to_string()))?;
            if available.is_empty() {
                if bytes.is_empty() {
                    return Ok(None);
                }
                return Err(EngineProcessErrorV1::InvalidFrame(
                    "engine frame ended without a newline".to_owned(),
                ));
            }
            let consumed = available
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(available.len(), |index| index + 1);
            if bytes.len() + consumed > MAX_ENGINE_FRAME_BYTES_V1 {
                return Err(EngineProcessErrorV1::FrameTooLarge);
            }
            bytes.extend_from_slice(&available[..consumed]);
            (consumed, available[consumed - 1] == b'\n')
        };
        reader.consume(consumed);
        if found_newline {
            return String::from_utf8(bytes)
                .map(Some)
                .map_err(|error| EngineProcessErrorV1::InvalidFrame(error.to_string()));
        }
    }
}
