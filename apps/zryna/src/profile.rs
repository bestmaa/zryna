use clap::ValueEnum;
use std::ffi::OsString;

use zryna_abi::ScalarValue;
use zryna_diagnostics::Diagnostic;
use zryna_driver::TargetSelection;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(super) enum CliProfile {
    #[value(name = "browser-component-v1")]
    BrowserComponentV1,
    #[value(name = "control-flow-v1")]
    ControlFlowV1,
    #[value(name = "data-ownership-v1")]
    DataOwnershipV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "lower")]
pub(super) enum CliTarget {
    JavaScript,
    WebAssembly,
    Native,
    Component,
    All,
}

impl From<CliTarget> for TargetSelection {
    fn from(value: CliTarget) -> Self {
        match value {
            CliTarget::JavaScript => Self::JavaScript,
            CliTarget::WebAssembly => Self::WebAssembly,
            CliTarget::Native => Self::Native,
            CliTarget::Component => Self::Component,
            CliTarget::All => Self::All,
        }
    }
}

pub(super) fn selects_typed_scalars(arguments: &[OsString]) -> bool {
    ["control-flow-v1", "data-ownership-v1"].iter().any(|profile| {
        arguments.windows(2).any(|pair| pair[0] == "--profile" && pair[1] == *profile)
            || arguments.iter().any(|argument| argument == format!("--profile={profile}").as_str())
    })
}

pub(super) fn parse_scalar_argument(value: &str) -> Result<ScalarValue, String> {
    let Some(decimal) = value.strip_prefix("i32:") else {
        return Err("expected canonical i32:<decimal> argument".to_owned());
    };
    let canonical = if decimal == "0" {
        true
    } else if let Some(digits) = decimal.strip_prefix('-') {
        !digits.is_empty()
            && !digits.starts_with('0')
            && digits.bytes().all(|byte| byte.is_ascii_digit())
    } else {
        !decimal.is_empty()
            && !decimal.starts_with('0')
            && decimal.bytes().all(|byte| byte.is_ascii_digit())
    };
    if !canonical {
        return Err("expected canonical signed base-ten i32 without whitespace or leading zeroes"
            .to_owned());
    }
    decimal
        .parse::<i32>()
        .map(ScalarValue::I32)
        .map_err(|_| "i32 argument is outside the signed 32-bit range".to_owned())
}

pub(super) fn parse_control_flow_argument(value: &str) -> Result<ScalarValue, String> {
    if let Some(boolean) = value.strip_prefix("bool:") {
        return match boolean {
            "true" => Ok(ScalarValue::Bool(true)),
            "false" => Ok(ScalarValue::Bool(false)),
            _ => Err("expected canonical bool:true or bool:false argument".to_owned()),
        };
    }
    parse_scalar_argument(value)
}

pub(super) fn default_stem(entrypoint: &str) -> String {
    std::path::Path::new(entrypoint)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_owned()
}

pub(super) fn cli_path_error() -> Diagnostic {
    Diagnostic::error(
        "ZRYNA-C1001",
        None,
        "CLI path could not be resolved to an existing absolute path",
        "pass an existing workspace root and direct Node.js executable path",
    )
}
