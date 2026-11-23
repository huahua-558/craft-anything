//! Command Code (`command-code acp`), driven over the Agent Client Protocol.
//!
//! Command Code's headless print mode (`-p --output-format json`) has no way to hand it an MCP
//! server, so a build there could not reach our tools at all. Its `acp` mode takes our server in
//! `session/new`, which is the only mode that gives the builder the engine's 300-odd tools.
//!
//! Sign-in stays in `command-code login`: Craft Anything checks that the account file exists and
//! never reads it. Reads are not gated by `session/request_permission` (only edits and commands
//! are), so Clean room, deep chat and questions are refused rather than promised and not kept;
//! only a deep build runs, as with Grok Build.

use super::*;
use std::path::{Path, PathBuf};

pub struct CommandCode;

impl AgentAdapter for CommandCode {
    fn ids(&self) -> &'static [&'static str] {
        &["command-code", "commandcode"]
    }

    /// The native runner refuses `requires_acp` before it asks for arguments: `-p` can't carry
    /// our tool server, and a bare `command-code "<prompt>"` opens the interactive session.
    fn args(&self, _cx: &ArgsCx) -> Vec<String> {
        Vec::new()
    }

    fn requires_acp(&self) -> bool {
        true
    }

    fn plain_fallback(&self) -> bool {
        false
    }

    fn has_model_catalog(&self) -> bool {
        true
    }

    /// Model-specific effort is advertised per session, so a model change has to be read again.
    fn model_catalog_depends_on_selection(&self) -> bool {
        true
    }

    fn model_catalog(
        &self,
        config: &BackendConfig,
        options: &BackendOptions,
        directory: Option<&Path>,
        stop: &Stop,
    ) -> Result<Vec<models::AgentModel>, ToolError> {
        let metadata = super::super::acp_model_metadata(config, options, directory, stop)?;
        Ok(catalogue(&metadata, options))
    }

    fn validate_model(&self, config: &BackendConfig, options: &BackendOptions, directory: Option<&Path>, stop: &Stop) -> Result<(), ToolError> {
        stop.check()?;
        self.validate_options(options)?;
        if options.cli_model.is_none() && options.cli_thinking.is_none() {
            return Ok(());
        }
        // A chosen level is sent as the nearest one Command Code accepts, so that is what the
        // catalogue offers; validating the level as written would refuse every level but those four.
        let sent = options.cli_thinking.map(accepted);
        let options = BackendOptions { cli_thinking: sent, ..options.clone() };
        models::validate(&options, &self.model_catalog(config, &options, directory, stop)?)
    }

    /// It can be held to reading: the ACP layer sends a question no tool server and refuses every
    /// permission request, and a question opens its own session rather than the builder's.
    fn answers_questions(&self) -> bool {
        true
    }

    fn model_in_use(&self, options: Option<&BackendOptions>) -> Option<String> {
        Some(
            options
                .and_then(|options| options.cli_model.as_deref().map(str::trim).filter(|model| !model.is_empty()).map(str::to_string))
                .unwrap_or_else(|| "Command Code default".into()),
        )
    }

    fn model_choices(&self, config: &BackendConfig, options: &BackendOptions) -> Result<Vec<String>, ToolError> {
        Ok(self.model_catalog(config, options, None, &Stop::default())?.into_iter().map(|model| model.id).collect())
    }

    fn check_tool_server(&self, stop: &CancelToken) -> Result<(), ToolError> {
        cli_can_start(craft_anything_platform::self_cli_path().as_deref(), stop)
    }

    /// Extra arguments would land before `acp`, where a flag such as `--yolo` or
    /// `--permission-mode dont-ask` would bypass the approval policy this backend promises.
    /// Everything supported is passed by [`acp_options`](Self::acp_options) and
    /// [`acp_session_config`](Self::acp_session_config).
    fn acp_options(&self, _options: &BackendOptions, _access: Access) -> Vec<String> {
        Vec::new()
    }

    fn validate_options(&self, options: &BackendOptions) -> Result<(), ToolError> {
        if options.args.is_empty() {
            Ok(())
        } else {
            Err(ToolError::invalid("Extra Command Code arguments could bypass Craft Anything's approval checks.")
                .with_remediation("Remove this backend's extra arguments. Set its program, model and thinking through the backend options instead."))
        }
    }

    /// Reads are not permission-gated in Command Code: a deep build is the only mode whose
    /// promise it keeps. Clean room can't be held back, and a chat or a question in Clean room
    /// would let it read the original. A deep chat would too, for the same reason.
    fn validate_access(&self, access: Access) -> Result<(), ToolError> {
        match access {
            Access::Build => Ok(()),
            Access::Analyse | Access::CleanRoomAnalyse | Access::CleanRoomBuild | Access::CleanRoomQuestion | Access::Question => {
                Err(ToolError::invalid("Command Code reads files without asking, so a Clean room run, a deep chat, or a question can't hold it back.")
                    .with_remediation(
                        "Use a deep build. Command Code asks before an edit or a command, which a deep build decides through the approval policy.",
                    ))
            }
        }
    }

    fn sign_in_fix(&self) -> Option<String> {
        Some(
            "Sign in with `command-code login` (or `cmd login`) in a terminal, then try again. Command Code keeps the sign-in; Craft Anything never sees it."
                .into(),
        )
    }

    /// Existence only: the account file holds the person's credentials and is not read. Missing is
    /// unknown rather than signed out, so a run still starts and reports the problem itself.
    fn signed_in(&self, _program: &str) -> Option<bool> {
        account_path(std::env::var_os("HOME").as_deref().map(Path::new), std::env::var_os("USERPROFILE").as_deref().map(Path::new))
            .filter(|p| p.is_file())
            .map(|_| true)
    }

    /// The session's own model and effort. Model first: switching it can reset the effort.
    fn acp_session_config(&self, options: &BackendOptions) -> Vec<(String, String)> {
        let mut config = Vec::new();
        if let Some(model) = options.cli_model.as_deref().map(str::trim).filter(|model| !model.is_empty()) {
            config.push(("model".into(), model.to_string()));
        }
        if let Some(effort) = chosen_effort(options) {
            config.push(("effort".into(), effort));
        }
        config
    }
}

/// `~/.commandcode/auth.json`: Command Code's own folder, where `login` keeps the account.
fn account_path(home: Option<&Path>, windows_home: Option<&Path>) -> Option<PathBuf> {
    let home = home.or(windows_home)?;
    Some(home.join(".commandcode").join("auth.json"))
}

/// Thinking depth, as the other agents choose it: an explicit picker choice, then a saved named
/// effort, then the older `effort` field. Only the levels Command Code accepts are sent; the rest
/// (`minimal`, `medium`, `xhigh`) map to the nearest one it has.
fn chosen_effort(options: &BackendOptions) -> Option<String> {
    let name = options
        .cli_thinking
        .map(|effort| effort.as_str())
        .or_else(|| options.cli_reasoning_effort.map(|effort| effort.as_str()))
        .or_else(|| options.effort.map(|effort| effort.as_str()))?;
    Some(sent_name(crate::Effort::parse(name)?).to_string())
}

/// The level Command Code is sent for a chosen one, as an [`Effort`]: the nearest at or below it,
/// so it never asks for more reasoning than was chosen. `off` is the absence of a depth, which no
/// [`Effort`] names, so it is [`Effort::None`] here.
fn accepted(depth: crate::Effort) -> crate::Effort {
    match depth {
        crate::Effort::None | crate::Effort::Minimal => crate::Effort::None,
        crate::Effort::Low => crate::Effort::Low,
        crate::Effort::Medium | crate::Effort::High => crate::Effort::High,
        crate::Effort::Xhigh | crate::Effort::Max => crate::Effort::Max,
    }
}

/// What Command Code calls the level [`accepted`] gives: `off`, `low`, `high` or `max`.
fn sent_name(depth: crate::Effort) -> &'static str {
    match depth {
        crate::Effort::None | crate::Effort::Minimal => "off",
        crate::Effort::Low => "low",
        crate::Effort::Medium | crate::Effort::High => "high",
        crate::Effort::Xhigh | crate::Effort::Max => "max",
    }
}

fn catalogue(metadata: &super::super::acp::ModelMetadata, options: &BackendOptions) -> Vec<models::AgentModel> {
    let active = options.cli_model.as_deref().map(str::trim).filter(|model| !model.is_empty()).unwrap_or(metadata.current_model.as_str());
    let thinking = sent_levels(&metadata.config_options);
    let mut ids = metadata.models.clone();
    if !metadata.current_model.is_empty() {
        ids.push(metadata.current_model.clone());
    }
    let mut ordered = Vec::new();
    for id in ids {
        if !id.is_empty() && !ordered.contains(&id) {
            ordered.push(id);
        }
    }
    ordered.into_iter().map(|id| models::AgentModel { name: id.clone(), thinking: if id == active { thinking.clone() } else { Vec::new() }, id }).collect()
}

/// The thinking levels this session offers, as the [`Effort`]s Command Code is sent for them. Its
/// option id is `effort`, not the `reasoning_effort` the shared tuning reads, and it accepts only
/// `off`, `low`, `high` and `max`: these are exactly what a chosen level is mapped onto and then
/// validated against. Its `default` means "leave the setting alone" and is not a depth.
fn sent_levels(config_options: &[serde_json::Value]) -> Vec<crate::Effort> {
    let offered = config_options
        .iter()
        .find(|option| option.get("id").and_then(serde_json::Value::as_str) == Some("effort"))
        .and_then(|option| option.get("options"))
        .and_then(serde_json::Value::as_array);
    let mut levels: Vec<crate::Effort> = Vec::new();
    for value in offered.into_iter().flatten().filter_map(|option| option.get("value").and_then(serde_json::Value::as_str)) {
        let Some(depth) = accepted_name(value) else { continue };
        if !levels.contains(&depth) {
            levels.push(depth);
        }
    }
    levels
}

/// The [`Effort`] Command Code's own name for a level stands for: `off`, `low`, `high`, `max`, and
/// its `default` (which means it is not choosing one). `None` for a name it doesn't offer.
fn accepted_name(name: &str) -> Option<crate::Effort> {
    match name.trim().to_ascii_lowercase().as_str() {
        "off" => Some(crate::Effort::None),
        "low" => Some(crate::Effort::Low),
        "high" => Some(crate::Effort::High),
        "max" => Some(crate::Effort::Max),
        other => crate::Effort::parse(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_code_is_acp_only_and_registered() {
        let adapter = AgentAdapters::built_in().for_agent("command-code");
        assert!(adapter.ids().contains(&"command-code"), "registered, not the plain adapter");
        assert!(adapter.requires_acp() && !adapter.plain_fallback());
        assert_eq!(super::super::acp::acp_args("command-code"), Some(&["acp"][..]));
        assert_eq!(display_for("command-code"), "Command Code");
    }

    /// `ids()` answers to two names, so a backend saved as either must start in ACP mode; without an
    /// entry the alias resolves to no ACP arguments and the run is refused.
    #[test]
    fn both_of_its_names_start_in_acp_mode() {
        for id in CommandCode.ids() {
            assert_eq!(super::super::acp::acp_args(id), Some(&["acp"][..]), "{id} has no ACP arguments");
            assert!(AgentAdapters::built_in().for_agent(id).requires_acp(), "{id} isn't the ACP adapter");
        }
        assert_eq!(super::super::acp::acp_args("commandcode"), Some(&["acp"][..]));
    }

    /// Reads are not permission-gated, so only a deep build is a promise Command Code keeps.
    #[test]
    fn only_a_deep_build_is_allowed() {
        assert!(CommandCode.validate_access(Access::Build).is_ok());
        for refused in [Access::Analyse, Access::CleanRoomAnalyse, Access::CleanRoomBuild, Access::CleanRoomQuestion, Access::Question] {
            let error = CommandCode.validate_access(refused).unwrap_err();
            assert!(error.message.contains("reads files without asking"), "{refused:?}: {}", error.message);
            assert!(error.remediation.is_some_and(|fix| fix.contains("deep build")), "{refused:?}");
        }
    }

    #[test]
    fn extra_arguments_are_refused() {
        assert!(CommandCode.validate_options(&BackendOptions::default()).is_ok());
        let options = BackendOptions { args: vec!["--yolo".into()], ..Default::default() };
        let error = CommandCode.validate_options(&options).unwrap_err();
        assert!(error.message.contains("Extra Command Code arguments"), "{}", error.message);
    }

    /// Command Code refuses every level but `off`, `low`, `high` and `max` with "Unknown effort".
    #[test]
    fn a_chosen_level_maps_onto_one_command_code_accepts() {
        let with = |options: BackendOptions| chosen_effort(&options);
        assert_eq!(with(BackendOptions { cli_thinking: Some(crate::Effort::Minimal), ..Default::default() }).as_deref(), Some("off"));
        assert_eq!(with(BackendOptions { cli_thinking: Some(crate::Effort::Medium), ..Default::default() }).as_deref(), Some("high"));
        assert_eq!(with(BackendOptions { cli_thinking: Some(crate::Effort::Xhigh), ..Default::default() }).as_deref(), Some("max"));
        assert_eq!(with(BackendOptions { cli_reasoning_effort: Some(crate::config::CliReasoningEffort::Low), ..Default::default() }).as_deref(), Some("low"));
        assert_eq!(with(BackendOptions { effort: Some(crate::Effort::Max), ..Default::default() }).as_deref(), Some("max"));
        assert_eq!(with(BackendOptions::default()), None, "nothing chosen: the CLI's own default");
    }

    /// A chosen level is sent as the nearest one Command Code accepts, so the catalogue must offer
    /// that one: validating the level as written would refuse `minimal`, `medium`, `xhigh` and
    /// `none`, which are all translated before anything is sent.
    #[test]
    fn every_chosen_level_is_accepted_once_translated() {
        let sent = sent_levels(&[serde_json::json!({"id":"effort","options":[
            {"value":"default"},{"value":"off"},{"value":"low"},{"value":"high"},{"value":"max"}
        ]})]);
        assert_eq!(sent, [crate::Effort::None, crate::Effort::Low, crate::Effort::High, crate::Effort::Max], "only the four depths it sends");
        for chosen in crate::Effort::CLI_ALL {
            let translated = accepted(chosen);
            assert!(sent.contains(&translated), "{chosen:?} translates to {translated:?}, which the catalogue must offer");
            assert_eq!(accepted_name(sent_name(translated)), Some(translated), "{chosen:?} round-trips");
        }
        assert_eq!(sent_levels(&[]), Vec::new(), "no option advertised: nothing to check against");
        assert_eq!(accepted_name("default"), None, "not a depth");
        assert_eq!(accepted_name("bogus"), None);
    }

    /// The model is set before the effort: switching model can reset the effort.
    #[test]
    fn the_session_sets_the_model_then_the_effort() {
        let options = BackendOptions { cli_model: Some("  claude-sonnet-5-5  ".into()), cli_thinking: Some(crate::Effort::High), ..Default::default() };
        assert_eq!(CommandCode.acp_session_config(&options), [("model".to_string(), "claude-sonnet-5-5".into()), ("effort".into(), "high".into())]);
        assert!(CommandCode.acp_session_config(&BackendOptions::default()).is_empty());
        assert!(CommandCode.acp_session_config(&BackendOptions { cli_model: Some(" ".into()), ..Default::default() }).is_empty());
    }

    #[test]
    fn the_account_file_is_where_command_code_keeps_it() {
        let home = Path::new("/home/someone");
        assert_eq!(account_path(Some(home), None), Some(PathBuf::from("/home/someone/.commandcode/auth.json")));
        assert_eq!(account_path(None, Some(Path::new("/Users/someone"))), Some(PathBuf::from("/Users/someone/.commandcode/auth.json")), "Windows' home");
        assert_eq!(account_path(None, None), None);
    }
}
