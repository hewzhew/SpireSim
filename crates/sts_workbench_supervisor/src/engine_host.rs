use crate::{
    EngineContractV1, EngineImageErrorV1, EngineImageStoreV1, EngineProcessErrorV1,
    EngineProcessV1, PrepareEngineOutcomeV1, RotationOutcomeV1, SupervisorCallOutcomeV1,
    SupervisorCallV1, SupervisorErrorV1, SupervisorModelV1, SUPERVISOR_PROTOCOL_V2,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineHostConfigurationV1 {
    repository: PathBuf,
    canonical_engine: PathBuf,
    image_store: PathBuf,
    java_jar: PathBuf,
    mod_jar: PathBuf,
    java_protocol: u16,
}

impl EngineHostConfigurationV1 {
    pub fn new(
        repository: impl Into<PathBuf>,
        canonical_engine: impl Into<PathBuf>,
        image_store: impl Into<PathBuf>,
        java_jar: impl Into<PathBuf>,
        mod_jar: impl Into<PathBuf>,
        java_protocol: u16,
    ) -> Result<Self, EngineHostErrorV1> {
        if java_protocol == 0 {
            return Err(EngineHostErrorV1::Configuration(
                "Java protocol must be nonzero".to_owned(),
            ));
        }
        Ok(Self {
            repository: repository.into(),
            canonical_engine: canonical_engine.into(),
            image_store: image_store.into(),
            java_jar: java_jar.into(),
            mod_jar: mod_jar.into(),
            java_protocol,
        })
    }

    fn contract(&self, image_id: &str) -> Result<EngineContractV1, EngineHostErrorV1> {
        Ok(EngineContractV1 {
            image_id: image_id.to_owned(),
            supervisor_protocol: SUPERVISOR_PROTOCOL_V2,
            java_build_id: file_sha256_upper(&self.java_jar)?,
            mod_build_id: file_sha256_upper(&self.mod_jar)?,
            minimum_java_protocol: self.java_protocol,
            maximum_java_protocol: self.java_protocol,
        })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum EngineHostErrorV1 {
    Configuration(String),
    Image(EngineImageErrorV1),
    Process(EngineProcessErrorV1),
    Supervisor(SupervisorErrorV1),
    DeferredBusy,
    MissingProcess,
    PhysicalProcessWithoutLease,
    ProcessAndSupervisor {
        process: EngineProcessErrorV1,
        supervisor: SupervisorErrorV1,
    },
}

impl fmt::Display for EngineHostErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for EngineHostErrorV1 {}

#[derive(Default)]
pub struct EngineHostV1 {
    process: Option<EngineProcessV1>,
}

impl EngineHostV1 {
    pub fn ensure_engine(
        &mut self,
        model: &mut SupervisorModelV1,
        configuration: &EngineHostConfigurationV1,
        timeout: Duration,
    ) -> Result<u64, EngineHostErrorV1> {
        let store = EngineImageStoreV1::new(&configuration.image_store);
        let image = store
            .materialize(&configuration.canonical_engine)
            .map_err(EngineHostErrorV1::Image)?;
        let contract = configuration.contract(image.image_id())?;

        if model
            .engine()
            .is_some_and(|current| current.contract == contract)
        {
            if let Some(process) = self.process.as_mut() {
                if process.require_running().is_ok() {
                    return Ok(model.engine().expect("checked Engine").epoch);
                }
            }
            let epoch = model.engine().expect("checked Engine").epoch;
            self.process.take();
            model
                .engine_disconnected(epoch)
                .map_err(EngineHostErrorV1::Supervisor)?;
        } else if model.engine().is_none() && self.process.is_some() {
            self.process.take();
        }

        let prepared = match model
            .prepare_engine(contract)
            .map_err(EngineHostErrorV1::Supervisor)?
        {
            PrepareEngineOutcomeV1::Prepared(prepared) => prepared,
            PrepareEngineOutcomeV1::DeferredBusy { .. } => {
                return Err(EngineHostErrorV1::DeferredBusy);
            }
        };
        let mut command = image
            .command(prepared.contract())
            .map_err(EngineHostErrorV1::Image)?;
        command
            .arg("--stdio")
            .arg("--repository")
            .arg(&configuration.repository)
            .arg("--java-jar")
            .arg(&configuration.java_jar)
            .arg("--mod-jar")
            .arg(&configuration.mod_jar)
            .stderr(Stdio::inherit());
        let mut candidate =
            EngineProcessV1::spawn(&mut command, prepared.contract().clone(), timeout)
                .map_err(EngineHostErrorV1::Process)?;
        candidate
            .bind(prepared.engine_epoch(), timeout)
            .map_err(EngineHostErrorV1::Process)?;
        let engine_epoch = match model
            .attach_prepared_engine(prepared)
            .map_err(EngineHostErrorV1::Supervisor)?
        {
            RotationOutcomeV1::Attached { engine_epoch, .. } => engine_epoch,
            RotationOutcomeV1::DeferredBusy { .. } => {
                return Err(EngineHostErrorV1::DeferredBusy);
            }
        };
        let previous = self.process.replace(candidate);
        if let Some(previous) = previous {
            if let Err(error) = previous.shutdown(timeout) {
                eprintln!("replaced Engine required forced cleanup: {error}");
            }
        }
        Ok(engine_epoch)
    }

    pub fn execute(
        &mut self,
        model: &mut SupervisorModelV1,
        configuration: &EngineHostConfigurationV1,
        input: Value,
        timeout: Duration,
    ) -> Result<Value, EngineHostErrorV1> {
        let engine_epoch = self.ensure_engine(model, configuration, timeout)?;
        let result = self
            .process
            .as_mut()
            .ok_or(EngineHostErrorV1::MissingProcess)?
            .execute(input, timeout);
        self.settle_execution(model, engine_epoch, result)
    }

    /// Executes one callback-capable job only on the already admitted Engine
    /// epoch. Busy jobs must never rotate and continue on a replacement image;
    /// a process failure is settled against this exact epoch instead.
    pub fn execute_bound_with_supervisor<Handle>(
        &mut self,
        model: &mut SupervisorModelV1,
        engine_epoch: u64,
        input: Value,
        timeout: Duration,
        handle: Handle,
    ) -> Result<Value, EngineHostErrorV1>
    where
        Handle: FnMut(SupervisorCallV1) -> Result<SupervisorCallOutcomeV1, String>,
    {
        match model.engine() {
            Some(engine) if engine.epoch == engine_epoch => {}
            Some(_) => {
                return Err(EngineHostErrorV1::Supervisor(
                    SupervisorErrorV1::EngineEpochMismatch,
                ));
            }
            None => {
                return Err(EngineHostErrorV1::Supervisor(
                    SupervisorErrorV1::EngineRequired,
                ));
            }
        }
        if self.process.is_none() {
            return match model.engine_disconnected(engine_epoch) {
                Ok(()) => Err(EngineHostErrorV1::MissingProcess),
                Err(error) => Err(EngineHostErrorV1::Supervisor(error)),
            };
        }
        let result = self
            .process
            .as_mut()
            .expect("bound Engine process exists")
            .execute_with_supervisor(input, timeout, handle);
        self.settle_execution(model, engine_epoch, result)
    }

    pub fn shutdown(
        &mut self,
        model: &mut SupervisorModelV1,
        timeout: Duration,
    ) -> Result<(), EngineHostErrorV1> {
        let Some(process) = self.process.take() else {
            return Ok(());
        };
        let epoch = model
            .engine()
            .map(|engine| engine.epoch)
            .ok_or(EngineHostErrorV1::PhysicalProcessWithoutLease)?;
        let process_result = process.shutdown(timeout);
        let supervisor_result = model.engine_disconnected(epoch);
        match (process_result, supervisor_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(EngineHostErrorV1::Process(error)),
            (Ok(()), Err(error)) => Err(EngineHostErrorV1::Supervisor(error)),
            (Err(process), Err(supervisor)) => Err(EngineHostErrorV1::ProcessAndSupervisor {
                process,
                supervisor,
            }),
        }
    }

    fn settle_execution(
        &mut self,
        model: &mut SupervisorModelV1,
        engine_epoch: u64,
        result: Result<Value, EngineProcessErrorV1>,
    ) -> Result<Value, EngineHostErrorV1> {
        match result {
            Ok(output) => Ok(output),
            Err(error @ EngineProcessErrorV1::Rejected { .. }) => {
                Err(EngineHostErrorV1::Process(error))
            }
            Err(process) => {
                self.process.take();
                match model.engine_disconnected(engine_epoch) {
                    Ok(()) => Err(EngineHostErrorV1::Process(process)),
                    Err(supervisor) => Err(EngineHostErrorV1::ProcessAndSupervisor {
                        process,
                        supervisor,
                    }),
                }
            }
        }
    }
}

fn file_sha256_upper(path: &Path) -> Result<String, EngineHostErrorV1> {
    let mut input = File::open(path).map_err(|error| {
        EngineHostErrorV1::Configuration(format!(
            "failed to open Engine contract input {}: {error}",
            path.display()
        ))
    })?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut input, &mut hasher).map_err(|error| {
        EngineHostErrorV1::Configuration(format!(
            "failed to hash Engine contract input {}: {error}",
            path.display()
        ))
    })?;
    Ok(format!("{:X}", hasher.finalize()))
}
