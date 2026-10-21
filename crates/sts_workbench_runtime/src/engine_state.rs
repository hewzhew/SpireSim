use crate::{runtime_manager, RootBindingV1, RuntimeStateV1};

/// Stable-actor mirror of the replaceable Engine's semantic Java context.
/// It never owns a Java process, bridge, sandbox, or repository containment,
/// and it stores no model-visible A/P/D/W view authority.
#[derive(Default)]
pub(super) struct WorkbenchEngineStateV1 {
    runtime_cache: RuntimeStateV1,
    java_runtime: Option<runtime_manager::JavaEngineRuntimeV1>,
}

impl WorkbenchEngineStateV1 {
    pub(super) fn runtime_cache(&self) -> &RuntimeStateV1 {
        &self.runtime_cache
    }

    pub(super) fn runtime_is_idle(&self) -> bool {
        self.runtime_cache.is_idle()
    }

    pub(super) fn runtime_is_neutral(&self) -> bool {
        self.runtime_cache.is_neutral()
    }

    pub(super) fn parked_root(
        &self,
        root_id: &str,
        action_space_id: &str,
    ) -> Option<RootBindingV1> {
        self.runtime_cache.parked_root(root_id, action_space_id)
    }

    pub(super) fn admit_runtime(
        &mut self,
        runtime: runtime_manager::JavaEngineRuntimeV1,
    ) -> Result<(), String> {
        if !self.runtime_cache.is_idle() || self.java_runtime.is_some() {
            return Err("runtime admission requires an idle Engine cache".to_owned());
        }
        self.java_runtime = Some(runtime);
        self.runtime_cache.reset_to_neutral();
        Ok(())
    }

    pub(super) fn park_runtime(&mut self, root: RootBindingV1) -> Result<(), String> {
        self.runtime_cache.park(root)
    }

    pub(super) fn reset_runtime(&mut self) -> Result<(), String> {
        self.java_runtime
            .as_mut()
            .ok_or_else(|| "native reset omitted its Engine Java context".to_owned())?
            .accept_native_reset();
        self.runtime_cache.reset_to_neutral();
        Ok(())
    }

    pub(super) fn clear_runtime(&mut self) {
        self.java_runtime = None;
        self.runtime_cache.clear();
    }

    pub(super) fn has_java_runtime(&self) -> bool {
        self.java_runtime.is_some()
    }

    pub(super) fn java_runtime_mut(&mut self) -> Option<&mut runtime_manager::JavaEngineRuntimeV1> {
        self.java_runtime.as_mut()
    }
}
